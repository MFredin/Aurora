//! Windows executor: registry writes, powercfg, System Restore and the journal folder ACL.
use super::Executor;
use forge_core::bloat::{BloatDb, Kind};
use forge_core::journal::Prior;
use forge_core::model::{FixStep, Hive, StartupLocation};
use std::os::windows::process::CommandExt;
use std::path::Path;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct WinExec {
    /// The signed-in user the app runs as. "HKCU" changes go to `HKEY_USERS\<sid>`, because the
    /// elevated helper may run as a different (administrator) account.
    user_sid: Option<String>,
    dev: bool,
}

impl WinExec {
    pub fn new(user_sid: Option<String>, dev: bool) -> Self {
        Self { user_sid, dev }
    }

    fn root(&self, hive: Hive) -> Result<RegKey, String> {
        match (hive, &self.user_sid) {
            (Hive::Hklm, _) => Ok(RegKey::predef(HKEY_LOCAL_MACHINE)),
            (Hive::Hkcu, Some(sid)) => RegKey::predef(HKEY_USERS)
                .open_subkey_with_flags(sid, KEY_READ | KEY_WRITE)
                .map_err(|e| format!("could not open the user's registry: {e}")),
            (Hive::Hkcu, None) if self.dev => Ok(RegKey::predef(HKEY_CURRENT_USER)),
            (Hive::Hkcu, None) => Err("the app did not say which user to change".into()),
        }
    }

    fn read_raw(&self, hive: Hive, key: &str, name: &str) -> Option<RegValue<'static>> {
        self.root(hive)
            .ok()?
            .open_subkey_with_flags(key, KEY_READ)
            .ok()?
            .get_raw_value(name)
            .ok()
    }

    fn write_raw(&self, hive: Hive, key: &str, name: &str, v: &RegValue) -> Result<(), String> {
        let (k, _) = self
            .root(hive)?
            .create_subkey(key)
            .map_err(|e| e.to_string())?;
        k.set_raw_value(name, v).map_err(|e| e.to_string())
    }

    fn delete(&self, hive: Hive, key: &str, name: &str) -> Result<(), String> {
        let Ok(k) = self.root(hive)?.open_subkey_with_flags(key, KEY_SET_VALUE) else {
            return Ok(()); // no key, nothing to delete
        };
        match k.delete_value(name) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }

    /// The command a Run-key startup item points at, read from the target user's hive.
    fn run_command(&self, name: &str, location: StartupLocation) -> Option<String> {
        let (hive, key) = match location {
            StartupLocation::HkcuRun => {
                (Hive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Run")
            }
            StartupLocation::HklmRun => {
                (Hive::Hklm, r"Software\Microsoft\Windows\CurrentVersion\Run")
            }
            StartupLocation::HklmRun32 => (
                Hive::Hklm,
                r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
            ),
            _ => return None,
        };
        self.root(hive)
            .ok()?
            .open_subkey_with_flags(key, KEY_READ)
            .ok()?
            .get_value::<String, _>(name)
            .ok()
    }
}

fn filetime_now() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    let mut ft = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn powercfg(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("powercfg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(if msg.is_empty() {
            "Windows refused to change the power plan.".into()
        } else {
            msg
        })
    }
}

fn binary(bytes: Vec<u8>) -> RegValue<'static> {
    RegValue {
        bytes: bytes.into(),
        vtype: REG_BINARY,
    }
}

impl Executor for WinExec {
    fn check(&mut self, step: &FixStep, db: &BloatDb) -> Result<(), String> {
        if let FixStep::DisableStartupItem { name, location } = step {
            // Re-check against the live command line, not just the name the app sent.
            if let Some(cmd) = self.run_command(name, *location) {
                if db.is_keep(Kind::Startup, &format!("{name} {cmd}")) {
                    return Err(format!("{name} is a protected Windows or driver component"));
                }
            } else if !matches!(
                location,
                StartupLocation::StartupFolderUser | StartupLocation::StartupFolderCommon
            ) {
                return Err(format!("{name} is no longer a startup item. Scan again."));
            }
        }
        Ok(())
    }

    fn prior(&mut self, step: &FixStep) -> Result<Prior, String> {
        Ok(match step {
            FixStep::DisableStartupItem { name, location }
            | FixStep::RestoreStartupItem { name, location, .. } => {
                let (hive, key) = location.approved_key();
                Prior::StartupApproved {
                    bytes: self.read_raw(hive, key, name).map(|v| v.bytes.to_vec()),
                }
            }
            FixStep::SetPowerPlan { .. } => {
                let guid = forge_scan::read_active_plan()
                    .or_else(|| {
                        powercfg(&["/getactivescheme"])
                            .ok()
                            .and_then(|o| forge_scan::parse::active_scheme(&o))
                            .map(|(g, _)| g)
                    })
                    .ok_or("Could not read the current power plan, so nothing was changed.")?;
                Prior::PowerPlan { guid }
            }
            FixStep::SetRegistryDword {
                hive, key, name, ..
            }
            | FixStep::DeleteRegistryValue { hive, key, name } => {
                match self.read_raw(*hive, key, name) {
                    Some(v) if v.vtype == REG_DWORD && v.bytes.len() >= 4 => Prior::Dword {
                        existed: true,
                        value: u32::from_le_bytes([v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3]]),
                    },
                    Some(_) => {
                        return Err(format!(
                            "{name} has an unexpected type, so Forge left it alone."
                        ));
                    }
                    None => Prior::Dword {
                        existed: false,
                        value: 0,
                    },
                }
            }
        })
    }

    fn apply(&mut self, step: &FixStep) -> Result<(), String> {
        match step {
            FixStep::DisableStartupItem { name, location } => {
                let (hive, key) = location.approved_key();
                let bytes = forge_scan::parse::disabled_bytes(filetime_now());
                self.write_raw(hive, key, name, &binary(bytes))
            }
            FixStep::RestoreStartupItem {
                name,
                location,
                prior,
            } => {
                let (hive, key) = location.approved_key();
                match prior {
                    Some(b) => self.write_raw(hive, key, name, &binary(b.clone())),
                    None => self.delete(hive, key, name),
                }
            }
            FixStep::SetPowerPlan { guid } => powercfg(&["/setactive", guid]).map(|_| ()),
            FixStep::SetRegistryDword {
                hive,
                key,
                name,
                value,
            } => {
                let v = RegValue {
                    bytes: value.to_le_bytes().to_vec().into(),
                    vtype: REG_DWORD,
                };
                self.write_raw(*hive, key, name, &v)
            }
            FixStep::DeleteRegistryValue { hive, key, name } => self.delete(*hive, key, name),
        }
    }

    fn prepare_journal_dir(&mut self, dir: &Path) -> Result<(), String> {
        if self.dev {
            return std::fs::create_dir_all(dir).map_err(|e| e.to_string());
        }
        secure_dir(dir)
    }

    fn restore_point(&mut self) -> Result<(bool, String), String> {
        restore_point()
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Creates `dir` so that only SYSTEM and Administrators can write it (users may read, for
/// History). If it already exists it must be owned by SYSTEM or Administrators; a folder planted
/// by a normal user is refused rather than trusted.
fn secure_dir(dir: &Path) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, EqualSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE, WinBuiltinAdministratorsSid, WinLocalSystemSid,
    };
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let path = wide(&dir.to_string_lossy());
    // Owner Administrators; protected DACL: SYSTEM and Administrators full, Users read/execute.
    let sddl = wide("O:BAG:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)");
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err("could not build the journal folder permissions".into());
        }
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let created = CreateDirectoryW(path.as_ptr(), &sa) != 0;
        let err = GetLastError();
        LocalFree(sd as _);
        if created {
            return Ok(());
        }
        if err != ERROR_ALREADY_EXISTS {
            return Err(format!("could not create {} (error {err})", dir.display()));
        }

        let mut owner: PSID = std::ptr::null_mut();
        let mut osd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let rc = GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut osd,
        );
        if rc != 0 {
            return Err(format!("could not check {} (error {rc})", dir.display()));
        }
        let is = |kind| {
            let mut buf = [0u8; SECURITY_MAX_SID_SIZE as usize];
            let mut len = buf.len() as u32;
            CreateWellKnownSid(
                kind,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            ) != 0
                && EqualSid(owner, buf.as_mut_ptr().cast()) != 0
        };
        let trusted = is(WinBuiltinAdministratorsSid) || is(WinLocalSystemSid);
        LocalFree(osd as _);
        if trusted {
            Ok(())
        } else {
            Err(format!(
                "The folder {} was not created by Forge, so Forge won't trust it with the undo journal. Delete it and try again.",
                dir.display()
            ))
        }
    }
}

/// Asks System Restore for a restore point. Windows makes at most one every 24 hours by default
/// and silently skips extra requests, which counts as success here.
fn restore_point() -> Result<(bool, String), String> {
    use windows_sys::Win32::Foundation::ERROR_SERVICE_DISABLED;
    use windows_sys::Win32::System::Com::{
        COINIT_MULTITHREADED, CoInitializeEx, CoInitializeSecurity, EOAC_NONE,
        RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows_sys::Win32::System::Restore::{
        BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW,
        SRSetRestorePointW, STATEMGRSTATUS,
    };
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32);
        // System Restore needs impersonation-level COM security in the calling process.
        CoInitializeSecurity(
            std::ptr::null_mut(),
            -1,
            std::ptr::null(),
            std::ptr::null(),
            RPC_C_AUTHN_LEVEL_DEFAULT,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            std::ptr::null(),
            EOAC_NONE as u32,
            std::ptr::null(),
        );
        let desc: Vec<u16> = "Forge: before changing settings".encode_utf16().collect();
        let mut text = [0u16; 256];
        text[..desc.len()].copy_from_slice(&desc);
        let info = RESTOREPOINTINFOW {
            dwEventType: BEGIN_SYSTEM_CHANGE,
            dwRestorePtType: MODIFY_SETTINGS,
            szDescription: text,
            ..Default::default()
        };
        let mut st = STATEMGRSTATUS::default();
        let ok = SRSetRestorePointW(&info, &mut st) != 0;
        let status = st.nStatus;
        if !ok {
            return Ok((
                false,
                if status == ERROR_SERVICE_DISABLED {
                    "System Restore is turned off on this PC, so no restore point was made. Forge can still undo its own changes from History.".into()
                } else {
                    format!(
                        "Windows could not make a restore point (error {status}). Forge can still undo its own changes from History."
                    )
                },
            ));
        }
        let end = RESTOREPOINTINFOW {
            dwEventType: END_SYSTEM_CHANGE,
            llSequenceNumber: st.llSequenceNumber,
            ..Default::default()
        };
        let mut st2 = STATEMGRSTATUS::default();
        SRSetRestorePointW(&end, &mut st2);
        Ok((true, "Restore point requested. Windows makes at most one a day, so an earlier one may be reused.".into()))
    }
}
