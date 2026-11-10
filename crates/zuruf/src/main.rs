mod doctor;
use clap::{Parser, Subcommand};
use zuruf::{Daemon, Error};
use zuruf_store::{Paths, StoreError};
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Daemon {
        #[arg(long)]
        seed_demo: bool,
    },
    Doctor,
    Status,
    Action {
        action_id: String,
    },
    Open {
        #[arg(long, value_parser = ["tasks", "sessions", "settings", "review"])]
        view: Option<String>,
    },
    Stop,
    T3 {
        #[command(subcommand)]
        command: T3Command,
    },
}
#[derive(Subcommand)]
enum T3Command {
    Pair {
        #[arg(long)]
        origin: String,
    },
}
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    if let Err(e) = run().await {
        tracing::error!(error=%e,"zuruf failed");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Error> {
    let args = Cli::parse();
    if matches!(args.command, Command::Doctor) {
        if !doctor::run() {
            std::process::exit(1);
        }
        return Ok(());
    }
    let paths = Paths::from_env()?;
    let port = std::env::var("ZURUF_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(47600);
    let base = format!("http://127.0.0.1:{port}");
    if let Command::T3 {
        command: T3Command::Pair { origin },
    } = &args.command
    {
        let setup = paths.clone();
        tokio::task::spawn_blocking(move || zuruf_store::Store::open(setup))
            .await
            .map_err(|e| Error::Join(e.to_string()))??;
        let mut adapter =
            zuruf_t3::T3Adapter::new("t3".into(), paths.state.join("t3-credentials.json"))
                .map_err(|e| Error::Join(e.to_string()))?;
        adapter
            .connect(origin)
            .await
            .map_err(|e| Error::Join(e.to_string()))?;
        zuruf_store::atomic_write(&paths.state.join("t3-origin"), origin.as_bytes())?;
        println!("T3 verbunden; Zugangsdaten privat gespeichert.");
        return Ok(());
    }
    if matches!(args.command, Command::Open { .. }) {
        ensure_daemon(&base).await?;
    }
    match args.command {
        Command::Daemon { seed_demo } => {
            let daemon = if std::env::var("ZURUF_FAKE_ADAPTERS").as_deref() == Ok("1") {
                Daemon::fake(paths).await?
            } else {
                Daemon::real(paths, port).await?
            };
            if seed_demo {
                daemon.seed_demo().await?;
            }
            if let Err(error) = daemon.recover().await {
                let _ = daemon.close().await;
                return Err(error);
            }
            let listener =
                match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        let _ = daemon.close().await;
                        return Err(StoreError::from(error).into());
                    }
                };
            tracing::info!(
                port = listener.local_addr().map_err(StoreError::from)?.port(),
                "daemon listening"
            );
            let result = axum::serve(listener, daemon.router())
                .with_graceful_shutdown({
                    let daemon = daemon.clone();
                    async move {
                        tokio::select! { _ = daemon.stopped() => {}, _ = async {
                            #[cfg(unix)]
                            {
                                if let Ok(mut term) = tokio::signal::unix::signal(
                                    tokio::signal::unix::SignalKind::terminate(),
                                ) {
                                    tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
                                } else {
                                    let _ = tokio::signal::ctrl_c().await;
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                let _ = tokio::signal::ctrl_c().await;
                            }
                        } => {} }
                        if let Err(e) = daemon.close().await {
                            tracing::warn!(error=%e, "shutdown cleanup failed");
                        }
                    }
                })
                .await;
            daemon.close().await?;
            result.map_err(StoreError::from)?;
        }
        command => {
            let token = tokio::fs::read_to_string(paths.state.join("token"))
                .await
                .map_err(StoreError::from)?;
            let client = reqwest::Client::new();
            let request = match &command {
                Command::Status => client.get(format!("{base}/api/v1/state")),
                Command::Action { action_id } => client
                    .post(format!("{base}/api/v1/actions/{action_id}"))
                    .json(&serde_json::json!({})),
                Command::Stop => client
                    .post(format!("{base}/api/v1/shutdown"))
                    .json(&serde_json::json!({})),
                Command::Open { .. } => client
                    .post(format!("{base}/api/v1/auth/code"))
                    .json(&serde_json::json!({})),
                Command::Daemon { .. } | Command::Doctor | Command::T3 { .. } => return Ok(()),
            };
            let response = request
                .bearer_auth(token.trim())
                .send()
                .await
                .map_err(|e| Error::Join(e.to_string()))?
                .error_for_status()
                .map_err(|e| Error::Join(e.to_string()))?
                .json::<serde_json::Value>()
                .await
                .map_err(|e| Error::Join(e.to_string()))?;
            if let Some(code) = response["code"].as_str() {
                let view = match &command {
                    Command::Open { view: Some(view) } => format!("&view={view}"),
                    _ => String::new(),
                };
                if std::env::var("ZURUF_FAKE_ADAPTERS").as_deref() == Ok("1") {
                    println!("{base}/#code={code}{view}");
                } else {
                    client.post(format!("{base}/api/v1/app/open"))
                        .bearer_auth(token.trim())
                        .json(&serde_json::json!({"view": match &command { Command::Open { view: Some(view) } => view.as_str(), _ => "tasks" }}))
                        .send().await.map_err(|e| Error::Join(e.to_string()))?
                        .error_for_status().map_err(|e| Error::Join(e.to_string()))?;
                    open_window(&paths, &format!("{base}/#code={code}{view}")).await?;
                }
            } else {
                if matches!(command, Command::Stop) {
                    let _ = tokio::process::Command::new("systemctl")
                        .args(["--user", "stop", "zuruf-app.service"])
                        .output()
                        .await;
                }
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
        }
    }
    Ok(())
}

async fn ensure_daemon(base: &str) -> Result<(), Error> {
    let client = reqwest::Client::new();
    if client.get(base).send().await.is_ok() {
        return Ok(());
    }
    let installed = tokio::process::Command::new("systemctl")
        .args(["--user", "cat", "zuruf.service"])
        .output()
        .await
        .map_err(StoreError::from)?
        .status
        .success();
    let mut command = if installed {
        let mut c = tokio::process::Command::new("systemctl");
        c.args(["--user", "start", "zuruf.service"]);
        c
    } else {
        let mut c = tokio::process::Command::new("systemd-run");
        c.args([
            "--user",
            "--collect",
            "--unit=zuruf-daemon",
            "--description=Zuruf daemon",
            "-p",
            "MemoryMax=3G",
            "-p",
            "MemorySwapMax=1G",
        ]);
        for name in [
            "ZURUF_PORT",
            "ZURUF_DATA_DIR",
            "ZURUF_STATE_DIR",
            "ZURUF_CONFIG_DIR",
            "ZURUF_FAKE_ADAPTERS",
            "T3_URL",
            "ZURUF_MASK_SELECTORS",
            "WAYLAND_DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
        ] {
            if let Ok(value) = std::env::var(name) {
                c.arg(format!("--setenv={name}={value}"));
            }
        }
        c.arg(std::env::current_exe().map_err(StoreError::from)?)
            .arg("daemon");
        c
    };
    if !command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(StoreError::from)?
        .success()
    {
        return Err(Error::Join("Daemon konnte nicht gestartet werden".into()));
    }
    for _ in 0..100 {
        if client.get(base).send().await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(Error::Join("Daemon-Start Timeout".into()))
}
async fn open_window(paths: &Paths, url: &str) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let profile = paths.data.join("ui-profile");
    tokio::fs::create_dir_all(&profile)
        .await
        .map_err(StoreError::from)?;
    tokio::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700))
        .await
        .map_err(StoreError::from)?;
    let unit = tokio::process::Command::new("systemctl")
        .args(["--user", "is-active", "zuruf-app.service"])
        .output()
        .await
        .map_err(StoreError::from)?;
    if unit.status.success() {
        let pid = tokio::process::Command::new("systemctl")
            .args([
                "--user",
                "show",
                "zuruf-app.service",
                "--property=MainPID",
                "--value",
            ])
            .output()
            .await
            .map_err(StoreError::from)?;
        let pid: u32 = String::from_utf8_lossy(&pid.stdout)
            .trim()
            .parse()
            .map_err(|_| Error::Join("App-PID fehlt".into()))?;
        let desktop =
            zuruf_hypr::Hyprland::detect(std::env::current_exe().map_err(StoreError::from)?)
                .await
                .map_err(|e| Error::Join(e.to_string()))?;
        let address = desktop
            .find_window(pid)
            .await
            .map_err(|e| Error::Join(e.to_string()))?
            .ok_or_else(|| Error::Join("App-Fenster fehlt".into()))?;
        desktop
            .focus(&address)
            .await
            .map_err(|e| Error::Join(e.to_string()))?;
        return Ok(());
    }
    let status = tokio::process::Command::new("systemd-run")
        .args([
            "--user",
            "--collect",
            "--unit=zuruf-app",
            "--description=Zuruf App",
            "-p",
            "MemoryMax=1G",
            "-p",
            "MemorySwapMax=512M",
            "/usr/bin/chromium",
            "--ozone-platform=wayland",
            "--class=zuruf",
            "--no-first-run",
            "--no-default-browser-check",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--app={url}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(StoreError::from)?;
    if !status.success() {
        return Err(Error::Join(
            "App-Fenster konnte nicht gestartet werden".into(),
        ));
    }
    Ok(())
}
