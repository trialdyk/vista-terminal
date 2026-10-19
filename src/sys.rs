//! System data from the Windows API: local time, battery, RAM, CPU, uptime
//! and console size. The non-Windows version is a stub so it still compiles.

pub struct LocalTime {
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[cfg(windows)]
mod imp {
    use super::{Battery, LocalTime};
    use std::mem::zeroed;
    use windows_sys::Win32::Foundation::{BOOL, FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, SetConsoleCtrlHandler, SetConsoleMode, WriteConsoleA,
        CONSOLE_SCREEN_BUFFER_INFO, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::SystemInformation::{
        GetLocalTime, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    pub fn local_time() -> LocalTime {
        let mut st: SYSTEMTIME = unsafe { zeroed() };
        unsafe { GetLocalTime(&mut st) };
        LocalTime {
            month: st.wMonth as u32,
            day: st.wDay as u32,
            weekday: st.wDayOfWeek as u32,
            hour: st.wHour as u32,
            minute: st.wMinute as u32,
        }
    }

    pub fn battery() -> Option<Battery> {
        let mut ps: SYSTEM_POWER_STATUS = unsafe { zeroed() };
        if unsafe { GetSystemPowerStatus(&mut ps) } == 0 {
            return None;
        }
        // 128 = no system battery, 255 = unknown
        if ps.BatteryFlag & 128 != 0 || ps.BatteryLifePercent > 100 {
            return None;
        }
        Some(Battery { percent: ps.BatteryLifePercent, charging: ps.ACLineStatus == 1 })
    }

    pub fn ram_percent() -> u32 {
        let mut ms: MEMORYSTATUSEX = unsafe { zeroed() };
        ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut ms) } == 0 {
            return 0;
        }
        ms.dwMemoryLoad
    }

    pub fn uptime_secs() -> u64 {
        (unsafe { GetTickCount64() }) / 1000
    }

    fn ft(f: &FILETIME) -> u64 {
        ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
    }

    /// (idle, total) in 100 ns units
    pub fn cpu_times() -> Option<(u64, u64)> {
        let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) =
            unsafe { (zeroed(), zeroed(), zeroed()) };
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        // kernel time already includes idle time
        Some((ft(&idle), ft(&kernel) + ft(&user)))
    }

    /// Visible window size as (columns, rows).
    pub fn console_size() -> Option<(u16, u16)> {
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = zeroed();
            if GetConsoleScreenBufferInfo(h, &mut info) == 0 {
                return None;
            }
            let w = info.srWindow;
            Some(((w.Right - w.Left + 1) as u16, (w.Bottom - w.Top + 1) as u16))
        }
    }

    /// Enable VT escape sequences. Fails quietly when stdout is not a console.
    pub fn prepare_console() {
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            if GetConsoleMode(h, &mut mode) != 0 {
                SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
            SetConsoleCtrlHandler(Some(on_ctrl), 1);
        }
    }

    /// The user's Documents folder, wherever it really is (OneDrive moves it).
    pub fn documents_dir() -> Option<std::path::PathBuf> {
        use windows_sys::Win32::System::Com::CoTaskMemFree;
        use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath};
        unsafe {
            let mut p: *mut u16 = std::ptr::null_mut();
            let ok = SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut p) >= 0 && !p.is_null();
            let path = ok.then(|| {
                let len = (0..).take_while(|&i| *p.add(i) != 0).count();
                let wide = std::slice::from_raw_parts(p, len);
                std::path::PathBuf::from(String::from_utf16_lossy(wide))
            });
            CoTaskMemFree(p as *const _);
            path
        }
    }

    /// Ctrl+C mid-animation: show the cursor again, then let the process exit.
    unsafe extern "system" fn on_ctrl(_kind: u32) -> BOOL {
        let seq = b"\x1b[0m\x1b[?25h\r\n";
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        WriteConsoleA(h, seq.as_ptr(), seq.len() as u32, std::ptr::null_mut(), std::ptr::null());
        0
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Battery, LocalTime};

    pub fn local_time() -> LocalTime {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        LocalTime {
            month: 1,
            day: 1,
            weekday: ((secs / 86_400 + 4) % 7) as u32,
            hour: ((secs / 3600) % 24) as u32,
            minute: ((secs / 60) % 60) as u32,
        }
    }
    pub fn battery() -> Option<Battery> {
        None
    }
    pub fn ram_percent() -> u32 {
        0
    }
    pub fn uptime_secs() -> u64 {
        0
    }
    pub fn cpu_times() -> Option<(u64, u64)> {
        None
    }
    pub fn console_size() -> Option<(u16, u16)> {
        None
    }
    pub fn prepare_console() {}
    pub fn documents_dir() -> Option<std::path::PathBuf> {
        None
    }
}

pub use imp::*;

/// Is a program with this name on the PATH?
pub fn on_path(name: &str) -> bool {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&file).is_file()))
}

/// CPU meter: take a sample at start-up, compute usage from the difference later.
pub struct CpuSampler(Option<(u64, u64)>);

impl CpuSampler {
    pub fn start() -> Self {
        CpuSampler(cpu_times())
    }

    pub fn usage(&self) -> Option<u32> {
        let (idle0, total0) = self.0?;
        let (idle1, total1) = cpu_times()?;
        let total = total1.saturating_sub(total0);
        if total == 0 {
            return None;
        }
        let busy = total.saturating_sub(idle1.saturating_sub(idle0));
        Some(((busy * 100) / total).min(100) as u32)
    }
}
