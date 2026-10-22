//! Talking to forge-helper. Release: start it elevated (one UAC prompt) and connect over a
//! private named pipe. Dev (`FORGE_HELPER_STDIO=1`) or non-Windows: run it unelevated on stdio.
use forge_core::ipc::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::time::Duration;

pub enum CallError {
    /// The request never reached the helper (it had already exited). Safe to retry.
    NotSent,
    /// The helper went away after receiving the request.
    Lost,
    Timeout,
}

pub struct HelperConn {
    writer: Box<dyn Write + Send>,
    lines: mpsc::Receiver<String>,
    kill: Option<Box<dyn FnOnce() + Send>>,
}

impl Drop for HelperConn {
    fn drop(&mut self) {
        // Close our write end first so an idle helper sees end-of-input and exits by itself.
        self.writer = Box::new(std::io::sink());
        if let Some(k) = self.kill.take() {
            k();
        }
    }
}

impl HelperConn {
    pub fn start() -> Result<Self, String> {
        let path = helper_path()?;
        if forge_core::paths::dev_mode() || !cfg!(windows) {
            return start_stdio(&path);
        }
        #[cfg(windows)]
        {
            pipe::start(&path)
        }
        #[cfg(not(windows))]
        unreachable!()
    }

    pub fn call(&mut self, req: &Request, timeout: Duration) -> Result<Response, CallError> {
        // Drop any stale reply before sending.
        while self.lines.try_recv().is_ok() {}
        let text = serde_json::to_string(req).map_err(|_| CallError::NotSent)?;
        writeln!(self.writer, "{text}")
            .and_then(|_| self.writer.flush())
            .map_err(|_| CallError::NotSent)?;
        match self.lines.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str(&line).map_err(|_| CallError::Lost),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(CallError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(CallError::Lost),
        }
    }
}

fn helper_path() -> Result<std::path::PathBuf, String> {
    // FORGE_HELPER overrides the path in dev, where the app and helper build into different
    // target dirs. Installed builds ship forge-helper.exe next to the app.
    if let Some(p) = std::env::var_os("FORGE_HELPER") {
        return Ok(p.into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) {
        "forge-helper.exe"
    } else {
        "forge-helper"
    };
    let p = exe.parent().ok_or("no exe dir")?.join(name);
    if p.exists() {
        Ok(p)
    } else {
        Err(format!(
            "Forge's helper is missing ({}). Reinstall Forge.",
            p.display()
        ))
    }
}

fn spawn_reader(r: impl std::io::Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(r).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn start_stdio(path: &std::path::Path) -> Result<HelperConn, String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .env(forge_core::paths::DEV_ENV, "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Forge's helper: {e}"))?;
    let stdin = child.stdin.take().ok_or("no helper stdin")?;
    let stdout = child.stdout.take().ok_or("no helper stdout")?;
    Ok(HelperConn {
        writer: Box::new(stdin),
        lines: spawn_reader(stdout),
        kill: Some(Box::new(move || {
            let _ = child.kill();
            let _ = child.wait();
        })),
    })
}

#[cfg(windows)]
mod pipe {
    use super::*;
    use std::hash::{BuildHasher, Hasher};
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_CANCELLED, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND,
        PIPE_ACCESS_OUTBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessId, OpenProcessToken, TerminateProcess,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// String SID of the user this app runs as, e.g. "S-1-5-21-...-1001".
    pub fn current_user_sid() -> Result<String, String> {
        unsafe {
            let mut tok: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
                return Err("could not read the current user".into());
            }
            let mut len = 0u32;
            GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(tok);
            if ok == 0 {
                return Err("could not read the current user".into());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
                return Err("could not read the current user".into());
            }
            let mut n = 0;
            while *s.add(n) != 0 {
                n += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s as _);
            Ok(sid)
        }
    }

    /// One-way pipe, one instance, local clients only. The default DACL lets only this user,
    /// SYSTEM and Administrators write; we also check the client is the process we started.
    fn create_pipe(name: &str, access: FILE_FLAGS_AND_ATTRIBUTES) -> Result<HANDLE, String> {
        let w = wide(name);
        let h = unsafe {
            CreateNamedPipeW(
                w.as_ptr(),
                access | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                std::ptr::null(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            Err("Could not open a channel to Forge's helper.".into())
        } else {
            Ok(h)
        }
    }

    fn close(h: HANDLE) {
        unsafe {
            CloseHandle(h);
        }
    }

    /// Two one-way pipes rather than one duplex pipe: Windows serializes synchronous I/O on a
    /// handle and its duplicates, so a blocked read on a shared duplex handle would also block
    /// every write, and both sides would deadlock.
    pub fn start(path: &std::path::Path) -> Result<HelperConn, String> {
        let nonce = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        let base = format!(r"\\.\pipe\forge-{}-{nonce:016x}", std::process::id());
        let (req_name, resp_name) = (format!("{base}-req"), format!("{base}-resp"));
        let sid = current_user_sid()?;
        let req = create_pipe(&req_name, PIPE_ACCESS_OUTBOUND)?;
        let resp = match create_pipe(&resp_name, PIPE_ACCESS_INBOUND) {
            Ok(h) => h,
            Err(e) => {
                close(req);
                return Err(e);
            }
        };

        let verb = wide("runas");
        let file = wide(&path.to_string_lossy());
        let params = wide(&format!("--pipe {base} --user-sid {sid}"));
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        // Blocks while Windows shows the UAC prompt.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let err = unsafe { GetLastError() };
            close(req);
            close(resp);
            return Err(if err == ERROR_CANCELLED {
                "Windows didn't get permission to make changes, so nothing was changed. Apply again and choose Yes in the Windows prompt.".into()
            } else {
                format!("Could not start Forge's helper (error {err}). Nothing was changed.")
            });
        }
        let proc_h = info.hProcess as usize;
        let kill_proc = move || unsafe {
            let h = proc_h as HANDLE;
            if !h.is_null() {
                TerminateProcess(h, 1);
                CloseHandle(h);
            }
        };

        // Wait for the helper to open both pipes (request first), without blocking forever.
        let (req_u, resp_u) = (req as usize, resp as usize);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let connect = |h: usize| {
                (unsafe { ConnectNamedPipe(h as HANDLE, std::ptr::null_mut()) } != 0)
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED
            };
            let ok = connect(req_u) && connect(resp_u);
            let _ = tx.send(ok);
        });
        let connected = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(ok) => ok,
            Err(_) => {
                // Unblock ConnectNamedPipe by connecting ourselves, then give up.
                let _ = std::fs::OpenOptions::new().read(true).open(&req_name);
                let _ = std::fs::OpenOptions::new().write(true).open(&resp_name);
                let _ = rx.recv_timeout(Duration::from_secs(2));
                false
            }
        };
        let expected = unsafe { GetProcessId(proc_h as HANDLE) };
        let client_is = |h: HANDLE| {
            let mut pid = 0u32;
            (unsafe { GetNamedPipeClientProcessId(h, &mut pid) } != 0) && pid == expected
        };
        if !(connected && client_is(req) && client_is(resp)) {
            close(req);
            close(resp);
            kill_proc();
            return Err("Forge's helper did not connect. Nothing was changed.".into());
        }

        let writer = unsafe { std::fs::File::from_raw_handle(req as _) };
        let reader = unsafe { std::fs::File::from_raw_handle(resp as _) };
        Ok(HelperConn {
            writer: Box::new(writer),
            lines: spawn_reader(reader),
            kill: Some(Box::new(move || {
                // Closing the request pipe makes the helper exit; terminate only if it hangs.
                let h = proc_h as HANDLE;
                unsafe {
                    if windows_sys::Win32::System::Threading::WaitForSingleObject(h, 1000) != 0 {
                        TerminateProcess(h, 1);
                    }
                    CloseHandle(h);
                }
            })),
        })
    }
}
