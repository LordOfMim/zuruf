//! Read-only checks; no configuration changes or downloads.
use std::{
    net::{SocketAddr, TcpStream},
    path::PathBuf,
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
    let mut ok = version("chromium");
    ok = version("voxtype") && ok;
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
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    let hypr = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .is_some_and(|s| runtime.join("hypr").join(s).join(".socket.sock").exists());
    println!("{}: Hyprland-Socket", if hypr { "ok" } else { "fehlt" });
    ok &= hypr;
    let pipe = runtime.join("pipewire-0").exists();
    println!("{}: PipeWire-Socket", if pipe { "ok" } else { "fehlt" });
    ok &= pipe;
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
    ok
}
