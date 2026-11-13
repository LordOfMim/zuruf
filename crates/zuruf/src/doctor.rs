//! Read-only checks; no configuration changes or downloads.
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::{
    net::{SocketAddr, TcpStream},
    process::Command,
    time::Duration,
};
fn version(program: &str) -> bool {
    match Command::new(program).arg("--version").output() {
        Ok(o) if o.status.success() => {
            println!(
                "ok: {program}: {}",
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .next()
                    .unwrap_or("vorhanden")
            );
            true
        }
        _ => {
            println!("fehlt: {program}");
            false
        }
    }
}
pub fn run() -> bool {
    // The browser is Zuruf's own shell with Chromium built in; it must sit next to its library.
    // On macOS the library is a framework in the app bundle's Frameworks folder.
    let shell = zuruf_browser::default_shell_path();
    let library = shell.parent().is_some_and(|directory| {
        directory
            .join(if cfg!(windows) {
                "libcef.dll"
            } else if cfg!(target_os = "macos") {
                "../Frameworks/Chromium Embedded Framework.framework"
            } else {
                "libcef.so"
            })
            .exists()
    });
    let mut ok = shell.exists() && library;
    println!(
        "{}: Browser-Shell {}{}",
        if ok { "ok" } else { "fehlt" },
        shell.display(),
        if shell.exists() && !library {
            " (Chromium-Laufzeit fehlt daneben)"
        } else {
            ""
        }
    );
    if cfg!(windows) {
        println!(
            "Hinweis: Diktat und desktopweite Hotkeys stehen unter Windows noch nicht bereit."
        );
    } else {
        if cfg!(target_os = "linux") {
            let _ = version("voxtype");
        }
        let whisper = Command::new("whisper-cli")
            .arg("--help")
            .output()
            .is_ok_and(|o| o.status.success());
        println!(
            "{}: whisper-cli (whisper.cpp, optional)",
            if whisper { "ok" } else { "fehlt" }
        );
        println!(
            "Hinweis: Diktat ist optional. Dienst und API-Schlüssel unter Einstellungen › Diktat wählen{}.",
            if cfg!(target_os = "macos") {
                "; desktopweite Hotkeys gibt es unter macOS nicht"
            } else {
                ""
            }
        );
    }
    ok = version("t3") && ok;
    let reachable = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 3773)),
        Duration::from_secs(2),
    )
    .is_ok();
    println!(
        "{}: T3 auf 127.0.0.1:3773",
        if reachable { "ok" } else { "fehlt" }
    );
    ok &= reachable;
    #[cfg(target_os = "linux")]
    {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_default();
        let hypr = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
            .is_some_and(|s| runtime.join("hypr").join(s).join(".socket.sock").exists());
        println!("{}: Hyprland-Socket", if hypr { "ok" } else { "fehlt" });
        let _ = hypr; // Optional: page hotkeys work without Hyprland.
        let pipe = runtime.join("pipewire-0").exists();
        println!("{}: PipeWire-Socket", if pipe { "ok" } else { "fehlt" });
        let _ = pipe; // Optional: typed comments work without PipeWire.
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let count = std::fs::read_dir(home.join(".local/share/voxtype/models"))
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| e.path().is_file())
                    .count()
            })
            .unwrap_or(0);
        println!(
            "Hinweis: {count} lokale Modelldateien im Voxtype-Standardverzeichnis. Ohne Modell in den Zuruf-Einstellungen gilt die Voxtype-Konfiguration (lokal oder Remote-Endpunkt)."
        );
        let memory = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemAvailable:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
        });
        println!(
            "Hinweis: Verfügbarer Arbeitsspeicher: {} MiB; Builds benötigen bis zu 3 GiB.",
            memory.unwrap_or(0) / 1024
        );
    }
    ok
}
