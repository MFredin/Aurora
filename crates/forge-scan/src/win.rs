//! Windows-only collectors. Everything here is cheap: registry reads and one firmware-table call.
//! No WMI, no PowerShell, no child processes except `powercfg`.
//! NOTE: written without a Windows compiler available; build on Windows and fix any signature drift.
use crate::{parse, smbios};
use forge_core::model::StartupLocation;
use forge_core::snapshot::{PowerPlan, Snapshot, StartupItem};
use std::os::windows::process::CommandExt;
use winreg::RegKey;
use winreg::enums::*;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn fill(s: &mut Snapshot) {
    s.startup = read_startup();
    s.appx = read_appx();
    s.gpus = read_gpus();
    s.active_power_plan = read_power_plan();
    let (laptop, on_battery) = power_status();
    s.is_laptop = laptop;
    s.on_battery = on_battery;
    if let Some(table) = read_smbios() {
        let (cfg, rated) = smbios::memory_speeds(&table);
        s.ram_configured_mhz = cfg;
        s.ram_rated_mhz = rated;
    }
}

fn read_startup() -> Vec<StartupItem> {
    let mut out = vec![];
    for (hive, loc) in [
        (HKEY_CURRENT_USER, StartupLocation::HkcuRun),
        (HKEY_LOCAL_MACHINE, StartupLocation::HklmRun),
    ] {
        let root = RegKey::predef(hive);
        let Ok(run) = root.open_subkey_with_flags(RUN, KEY_READ) else {
            continue;
        };
        let approved = root.open_subkey_with_flags(APPROVED, KEY_READ).ok();
        for (name, _) in run.enum_values().flatten() {
            let command: String = run.get_value(&name).unwrap_or_default();
            let bytes = approved
                .as_ref()
                .and_then(|a| a.get_raw_value(&name).ok())
                .map(|v| v.bytes.to_vec());
            out.push(StartupItem {
                enabled: parse::startup_enabled(bytes.as_deref()),
                name,
                command,
                location: loc,
            });
        }
    }
    out
}

fn read_appx() -> Vec<String> {
    let path = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_READ) else {
        return vec![];
    };
    let mut names: Vec<String> = k
        .enum_keys()
        .flatten()
        .map(|key| parse::appx_name(&key).to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn read_gpus() -> Vec<String> {
    let class = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(class, KEY_READ) else {
        return vec![];
    };
    k.enum_keys()
        .flatten()
        .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|n| {
            k.open_subkey(&n)
                .ok()?
                .get_value::<String, _>("DriverDesc")
                .ok()
        })
        .collect()
}

fn read_power_plan() -> Option<PowerPlan> {
    let out = std::process::Command::new("powercfg")
        .arg("/getactivescheme")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let (guid, name) = parse::active_scheme(&String::from_utf8_lossy(&out.stdout))?;
    Some(PowerPlan { guid, name })
}

fn power_status() -> (bool, bool) {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut st: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut st) } == 0 {
        return (false, false);
    }
    let has_battery = st.BatteryFlag != 128 && st.BatteryFlag != 255;
    (has_battery, has_battery && st.ACLineStatus == 0)
}

fn read_smbios() -> Option<Vec<u8>> {
    use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;
    const RSMB: u32 = 0x5253_4D42;
    unsafe {
        let size = GetSystemFirmwareTable(RSMB, 0, std::ptr::null_mut(), 0);
        if size < 8 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let got = GetSystemFirmwareTable(RSMB, 0, buf.as_mut_ptr().cast(), size);
        if got == 0 || got as usize > buf.len() {
            return None;
        }
        buf.truncate(got as usize);
        Some(buf.split_off(8)) // drop RawSMBIOSData header
    }
}

/// While alive, the current thread runs at background CPU, I/O and memory priority.
pub struct BackgroundGuard;
impl BackgroundGuard {
    pub fn enter() -> Self {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
        };
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN as _);
        }
        BackgroundGuard
    }
}
impl Drop for BackgroundGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_END,
        };
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_END as _);
        }
    }
}
