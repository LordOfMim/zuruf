#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let result = (|| -> std::io::Result<bool> {
        let current = std::env::current_exe()?;
        let filename = if cfg!(windows) { "zuruf.exe" } else { "zuruf" };
        let nested = current
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("bin")
            .join(filename);
        let binary = if nested.is_file() {
            nested
        } else {
            current.with_file_name(filename)
        };
        let mut command = std::process::Command::new(binary);
        zuruf_platform::background(&mut command);
        Ok(command
            .arg("open")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success())
    })();
    if !matches!(result, Ok(true)) {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
            let message: Vec<u16> = "Zuruf konnte nicht gestartet werden. Bitte prüfe mit „zuruf doctor“ die Installation und das Protokoll unter %LOCALAPPDATA%\\Zuruf\\state\\daemon.log.".encode_utf16().chain(Some(0)).collect();
            let title: Vec<u16> = "Zuruf".encode_utf16().chain(Some(0)).collect();
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
        #[cfg(target_os = "macos")]
        {
            // Started from Finder there is no terminal to read; show a dialog instead.
            let script = "display alert \"Zuruf konnte nicht gestartet werden\" message \"Bitte im Terminal „zuruf doctor“ ausführen. Das Protokoll liegt unter ~/Library/Application Support/Zuruf/state/daemon.log.\" as critical";
            let _ = std::process::Command::new("osascript")
                .args(["-e", script])
                .status();
        }
        #[cfg(not(windows))]
        eprintln!("Zuruf konnte nicht gestartet werden; bitte zuruf doctor ausführen.");
        std::process::exit(1);
    }
}
