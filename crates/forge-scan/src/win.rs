//! Windows-only collectors. Everything here is cheap: registry reads, a few Win32 queries and one
//! firmware-table call. No WMI, no PowerShell, no child processes.
use crate::{parse, smbios};
use forge_core::ipc;
use forge_core::model::{Hive, StartupLocation};
use forge_core::snapshot::{DiskKind, Display, GpuDriver, PowerPlan, Snapshot, StartupItem};
use std::path::PathBuf;
use winreg::RegKey;
use winreg::enums::*;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const POWER_SCHEMES: &str = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";

pub fn fill(s: &mut Snapshot) {
    s.startup = read_startup();
    s.appx = read_appx();
    let drivers = read_gpu_drivers();
    s.gpus = parse::clean_gpus(drivers.iter().map(|g| g.name.clone()).collect());
    s.gpu_drivers = drivers
        .into_iter()
        .filter(|g| s.gpus.contains(&g.name))
        .fold(vec![], |mut acc: Vec<GpuDriver>, g| {
            if !acc.iter().any(|a| a.name == g.name) {
                acc.push(g);
            }
            acc
        });
    s.displays = read_displays();
    let (plans, active) = read_power_plans();
    s.active_power_plan = active.and_then(|g| plans.iter().find(|p| p.guid == g).cloned());
    s.power_plans = plans;
    let (has_battery, on_battery) = power_status();
    s.on_battery = on_battery;
    let info = read_smbios().map(|t| smbios::parse(&t)).unwrap_or_default();
    s.ram_configured_mts = info.configured_mts;
    s.ram_rated_mts = info.rated_mts;
    s.ram_modules = info.modules;
    // Chassis type is the better signal (a desktop on a UPS reports a battery).
    s.is_laptop = info.portable.unwrap_or(has_battery);
    s.game_dvr_background =
        read_dword(Hive::Hkcu, ipc::GAME_DVR, "HistoricalCaptureEnabled").map(|v| v == 1);
    s.game_mode = read_dword(Hive::Hkcu, ipc::GAME_BAR, "AutoGameModeEnabled").map(|v| v == 1);
    s.mouse_acceleration = read_text(Hive::Hkcu, ipc::MOUSE, "MouseSpeed").map(|v| v.trim() != "0");
    s.windows_suggestions_on = ipc::SUGGESTION_VALUES
        .iter()
        .filter(|n| read_dword(Hive::Hkcu, ipc::CONTENT_DELIVERY, n) == Some(1))
        .map(|n| n.to_string())
        .collect();
    s.storage_sense = read_dword(Hive::Hkcu, ipc::STORAGE_SENSE, "01").map(|v| v == 1);
    s.hags = match read_dword(Hive::Hklm, ipc::GRAPHICS_DRIVERS, "HwSchMode") {
        Some(2) => Some(true),
        Some(1) => Some(false),
        _ => None,
    };
    s.dx_user_prefs = Some(
        read_text(Hive::Hkcu, ipc::DIRECTX_PREFS, ipc::DIRECTX_PREFS_VALUE).unwrap_or_default(),
    );
    s.power_mode = read_power_mode();
    s.reboot_pending = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
    ]
    .iter()
    .any(|k| {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(k, KEY_READ)
            .is_ok()
    });
    s.recycle_bin_bytes = recycle_bin_bytes();
    for d in &mut s.disks {
        if let Some(k) = disk_kind(&d.mount) {
            d.kind = k;
        }
    }
}

fn hkey(h: Hive) -> RegKey {
    RegKey::predef(match h {
        Hive::Hkcu => HKEY_CURRENT_USER,
        Hive::Hklm => HKEY_LOCAL_MACHINE,
    })
}

fn read_dword(hive: Hive, key: &str, name: &str) -> Option<u32> {
    hkey(hive)
        .open_subkey_with_flags(key, KEY_READ)
        .ok()?
        .get_value::<u32, _>(name)
        .ok()
}

fn read_text(hive: Hive, key: &str, name: &str) -> Option<String> {
    hkey(hive)
        .open_subkey_with_flags(key, KEY_READ)
        .ok()?
        .get_value::<String, _>(name)
        .ok()
}

fn approved_bytes(location: StartupLocation, name: &str) -> Option<Vec<u8>> {
    let (hive, key) = location.approved_key();
    hkey(hive)
        .open_subkey_with_flags(key, KEY_READ)
        .ok()?
        .get_raw_value(name)
        .ok()
        .map(|v| v.bytes.to_vec())
}

fn startup_folder(location: StartupLocation) -> Option<PathBuf> {
    let (var, rest) = match location {
        StartupLocation::StartupFolderUser => {
            ("APPDATA", r"Microsoft\Windows\Start Menu\Programs\Startup")
        }
        StartupLocation::StartupFolderCommon => (
            "ProgramData",
            r"Microsoft\Windows\Start Menu\Programs\StartUp",
        ),
        _ => return None,
    };
    std::env::var_os(var).map(|b| PathBuf::from(b).join(rest))
}

fn run_key(location: StartupLocation) -> Option<(Hive, &'static str)> {
    match location {
        StartupLocation::HkcuRun => Some((Hive::Hkcu, RUN)),
        StartupLocation::HklmRun => Some((Hive::Hklm, RUN)),
        StartupLocation::HklmRun32 => Some((Hive::Hklm, RUN32)),
        _ => None,
    }
}

const LOCATIONS: [StartupLocation; 5] = [
    StartupLocation::HkcuRun,
    StartupLocation::HklmRun,
    StartupLocation::HklmRun32,
    StartupLocation::StartupFolderUser,
    StartupLocation::StartupFolderCommon,
];

fn read_startup() -> Vec<StartupItem> {
    let mut out = vec![];
    for loc in LOCATIONS {
        for (name, command) in entries(loc) {
            out.push(StartupItem {
                enabled: parse::startup_enabled(approved_bytes(loc, &name).as_deref()),
                name,
                command,
                location: loc,
                protected: false,
                note: None,
            });
        }
    }
    out
}

/// (name, command) pairs at one startup location. For the Startup folders the name is the file
/// name, which is also the StartupApproved value name.
fn entries(loc: StartupLocation) -> Vec<(String, String)> {
    if let Some((hive, key)) = run_key(loc) {
        let Ok(run) = hkey(hive).open_subkey_with_flags(key, KEY_READ) else {
            return vec![];
        };
        return run
            .enum_values()
            .flatten()
            .map(|(name, _)| {
                let command: String = run.get_value(&name).unwrap_or_default();
                (name, command)
            })
            .collect();
    }
    let Some(dir) = startup_folder(loc) else {
        return vec![];
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (!name.eq_ignore_ascii_case("desktop.ini"))
                .then(|| (name, e.path().to_string_lossy().to_string()))
        })
        .collect()
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

fn read_gpu_drivers() -> Vec<GpuDriver> {
    let class = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(class, KEY_READ) else {
        return vec![];
    };
    k.enum_keys()
        .flatten()
        .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|n| {
            let sub = k.open_subkey(&n).ok()?;
            Some(GpuDriver {
                name: sub.get_value::<String, _>("DriverDesc").ok()?,
                version: sub
                    .get_value::<String, _>("DriverVersion")
                    .unwrap_or_default(),
                date: sub
                    .get_value::<String, _>("DriverDate")
                    .ok()
                    .and_then(|d| parse::driver_date(&d)),
            })
        })
        .collect()
}

/// Installed plans and the active plan's GUID, from the registry (no powercfg process).
fn read_power_plans() -> (Vec<PowerPlan>, Option<String>) {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(POWER_SCHEMES, KEY_READ)
    else {
        return (vec![], None);
    };
    let active = k
        .get_value::<String, _>("ActivePowerScheme")
        .ok()
        .map(|g| g.to_lowercase());
    let plans = k
        .enum_keys()
        .flatten()
        .filter(|g| forge_core::power::is_guid(g))
        .map(|guid| {
            let name = k
                .open_subkey(&guid)
                .ok()
                .and_then(|p| p.get_value::<String, _>("FriendlyName").ok())
                .map(|n| parse::power_friendly_name(&n))
                .unwrap_or_else(|| forge_core::power::name(&guid.to_lowercase()).to_string());
            PowerPlan {
                guid: guid.to_lowercase(),
                name,
            }
        })
        .collect();
    (plans, active)
}

/// Windows 11 power mode on mains power. Windows 11 always offers power modes with the Balanced
/// plan; an absent value means the default (Balanced). None on Windows 10 and earlier.
pub fn read_power_mode() -> Option<String> {
    let build: u32 = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        .ok()?
        .get_value::<String, _>("CurrentBuildNumber")
        .ok()?
        .parse()
        .ok()?;
    if build < 22000 {
        return None;
    }
    let mode = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(POWER_SCHEMES, KEY_READ)
        .ok()
        .and_then(|k| k.get_value::<String, _>("ActiveOverlayAcPowerScheme").ok())
        .map(|g| g.to_lowercase())
        .filter(|g| forge_core::power::is_guid(g));
    Some(mode.unwrap_or_else(|| forge_core::power::MODE_BALANCED.to_string()))
}

fn recycle_bin_bytes() -> Option<u64> {
    use windows_sys::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    (unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) } == 0)
        .then(|| u64::try_from(info.i64Size).unwrap_or(0))
}

/// Empties the signed-in user's Recycle Bin on every drive, without prompts.
pub fn empty_recycle_bin() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW,
    };
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    // S_OK, or E_UNEXPECTED when the bin was already empty.
    if hr == 0 || hr as u32 == 0x8000_FFFF {
        Ok(())
    } else {
        Err(format!(
            "Windows could not empty the Recycle Bin (0x{:08X}).",
            hr as u32
        ))
    }
}

/// The active power plan GUID. Used by the helper to record what to restore.
pub fn read_active_plan() -> Option<String> {
    read_power_plans().1
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

fn read_displays() -> Vec<Display> {
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS,
        EnumDisplayDevicesW, EnumDisplaySettingsW,
    };
    let mut out = vec![];
    let mut i = 0u32;
    loop {
        let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd, 0) } == 0 {
            break;
        }
        i += 1;
        if dd.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let dev = dd.DeviceName.as_ptr();
        let new_mode = || {
            let mut m: DEVMODEW = unsafe { std::mem::zeroed() };
            m.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            m
        };
        let mut cur = new_mode();
        if unsafe { EnumDisplaySettingsW(dev, ENUM_CURRENT_SETTINGS, &mut cur) } == 0 {
            continue;
        }
        let mut modes = vec![];
        let mut j = 0u32;
        loop {
            let mut m = new_mode();
            if unsafe { EnumDisplaySettingsW(dev, j, &mut m) } == 0 || j > 4096 {
                break;
            }
            modes.push((m.dmPelsWidth, m.dmPelsHeight, m.dmDisplayFrequency));
            j += 1;
        }
        let (w, h, hz) = (cur.dmPelsWidth, cur.dmPelsHeight, cur.dmDisplayFrequency);
        out.push(Display {
            name: format!("Display {}", out.len() + 1),
            width: w,
            height: h,
            current_hz: hz,
            max_hz: parse::best_refresh(w, h, &modes).unwrap_or(hz).max(hz),
        });
    }
    out
}

/// NVMe / SSD / HDD for a drive letter, from the storage stack (sysinfo often says Unknown).
fn disk_kind(mount: &str) -> Option<DiskKind> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusTypeNvme, CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
        STORAGE_ADAPTER_DESCRIPTOR, STORAGE_PROPERTY_QUERY, StorageAdapterProperty,
        StorageDeviceSeekPenaltyProperty,
    };
    let letter = mount.trim_end_matches(['\\', ':']);
    if letter.len() != 1 {
        return None;
    }
    let path: Vec<u16> = format!(r"\\.\{letter}:")
        .encode_utf16()
        .chain([0])
        .collect();
    // Access 0: query metadata only, no admin rights needed.
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    fn query<T>(h: windows_sys::Win32::Foundation::HANDLE, id: i32) -> Option<T> {
        let mut q: STORAGE_PROPERTY_QUERY = unsafe { std::mem::zeroed() };
        q.PropertyId = id;
        q.QueryType = PropertyStandardQuery;
        let mut out: T = unsafe { std::mem::zeroed() };
        let mut got = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&q as *const STORAGE_PROPERTY_QUERY).cast(),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                (&mut out as *mut T).cast(),
                std::mem::size_of::<T>() as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        };
        (ok != 0 && got > 0).then_some(out)
    }
    let adapter: Option<STORAGE_ADAPTER_DESCRIPTOR> = query(h, StorageAdapterProperty);
    let seek: Option<DEVICE_SEEK_PENALTY_DESCRIPTOR> = query(h, StorageDeviceSeekPenaltyProperty);
    unsafe { CloseHandle(h) };
    if adapter.is_some_and(|a| a.BusType as i32 == BusTypeNvme) {
        return Some(DiskKind::Nvme);
    }
    seek.map(|s| {
        if s.IncursSeekPenalty {
            DiskKind::Hdd
        } else {
            DiskKind::Ssd
        }
    })
}

/// Private working set of a process in bytes (Task Manager's "Memory" column).
pub fn private_working_set(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c = PROCESS_MEMORY_COUNTERS_EX2 {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            ..Default::default()
        };
        let ok = GetProcessMemoryInfo(
            h,
            (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX2).cast::<PROCESS_MEMORY_COUNTERS>(),
            c.cb,
        );
        CloseHandle(h);
        (ok != 0).then_some(c.PrivateWorkingSetSize as u64)
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
