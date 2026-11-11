#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let result = (|| -> std::io::Result<bool> {
        let binary = std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "zuruf.exe"
        } else {
            "zuruf"
        });
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
        #[cfg(not(windows))]
        eprintln!("Zuruf konnte nicht gestartet werden; bitte zuruf doctor ausführen.");
        std::process::exit(1);
    }
}
