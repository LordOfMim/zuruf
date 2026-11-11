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
        let client = reqwest::Client::new();
        if let Ok(token) = tokio::fs::read_to_string(paths.state.join("token")).await {
            match client
                .post(format!("{base}/api/v1/t3/pair"))
                .bearer_auth(token.trim())
                .json(&serde_json::json!({"origin":origin}))
                .send()
                .await
            {
                Ok(response) => {
                    if !response.status().is_success() {
                        return Err(Error::Join("T3-Paarung im Daemon fehlgeschlagen.".into()));
                    }
                    println!("T3 live verbunden; Zugangsdaten privat gespeichert.");
                    return Ok(());
                }
                Err(error) if error.is_connect() => (),
                Err(_) => {
                    return Err(Error::Join(
                        "Daemon nicht erreichbar; Paarungszustand prüfen.".into(),
                    ));
                }
            }
        }
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
        ensure_daemon(&base, &paths).await?;
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
            let listener = match bind(port).await {
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
            let server = axum::serve(listener, daemon.router()).with_graceful_shutdown({
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
            });
            // Serve the panel before recovery launches its browser windows. Otherwise a
            // restored panel can load an error page while the HTTP port is still closed.
            let recovery = async {
                tokio::select! {
                    result = daemon.recover() => result,
                    _ = daemon.stopped() => Ok(()),
                }
            };
            let result = tokio::try_join!(recovery, async {
                server
                    .await
                    .map_err(|error| Error::from(StoreError::from(error)))
            });
            daemon.close().await?;
            result?;
        }
        command => {
            let token = tokio::fs::read_to_string(paths.state.join("token"))
                .await
                .map_err(StoreError::from)?;
            let client = reqwest::Client::new();
            // With fake adapters there are no windows; print a sign-in link instead.
            let fake = std::env::var("ZURUF_FAKE_ADAPTERS").as_deref() == Ok("1");
            let view = match &command {
                Command::Open { view: Some(view) } => view.as_str(),
                _ => "tasks",
            };
            let request = match &command {
                Command::Status => client.get(format!("{base}/api/v1/state")),
                Command::Action { action_id } => client
                    .post(format!("{base}/api/v1/actions/{action_id}"))
                    .json(&serde_json::json!({})),
                Command::Stop => client
                    .post(format!("{base}/api/v1/shutdown"))
                    .json(&serde_json::json!({})),
                Command::Open { .. } if fake => client
                    .post(format!("{base}/api/v1/auth/code"))
                    .json(&serde_json::json!({})),
                // The daemon shows the app: docked in the session window or in its own.
                Command::Open { .. } => client
                    .post(format!("{base}/api/v1/app/open"))
                    .json(&serde_json::json!({ "view": view })),
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
            match response["code"].as_str() {
                Some(code) => println!("{base}/#code={code}&view={view}"),
                None => println!("{}", serde_json::to_string_pretty(&response)?),
            }
        }
    }
    Ok(())
}

/// Listen on the daemon's loopback port. The port lies in the range the system also hands to
/// outgoing connections, and a daemon that has just stopped may not have let go of it yet;
/// that is worth a wait of up to fifteen seconds rather than a failed start.
async fn bind(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    let mut attempts = 0;
    loop {
        match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse && attempts < 60 => {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            result => return result,
        }
    }
}

async fn ensure_daemon(base: &str, paths: &Paths) -> Result<(), Error> {
    let client = reqwest::Client::new();
    if client.get(base).send().await.is_ok() {
        return Ok(());
    }
    let binary = std::env::current_exe().map_err(StoreError::from)?;
    // Installer and desktop smoke tests have their own state and port. They must
    // never start the user's installed service, which has a different environment.
    let isolated = std::env::var("ZURUF_INSTALL_TEST").as_deref() == Ok("1");
    let systemd = !isolated
        && cfg!(target_os = "linux")
        && std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|runtime| {
            std::path::Path::new(&runtime)
                .join("systemd/private")
                .exists()
        });
    let started = if systemd {
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
                "ZURUF_SHELL",
                "ZURUF_SHELL_ARGS",
                "T3_URL",
                "ZURUF_MASK_SELECTORS",
                "WAYLAND_DISPLAY",
                "DISPLAY",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ] {
                if let Ok(value) = std::env::var(name) {
                    c.arg(format!("--setenv={name}={value}"));
                }
            }
            c.arg(&binary).arg("daemon");
            c
        };
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map_err(StoreError::from)?
            .success()
    } else {
        // No systemd user session: the daemon runs as a plain background process. There is
        // no journal either, so it keeps its own log.
        tokio::fs::create_dir_all(&paths.state)
            .await
            .map_err(StoreError::from)?;
        let log =
            std::fs::File::create(paths.state.join("daemon.log")).map_err(StoreError::from)?;
        tokio::process::Command::new(&binary)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(log)
            .spawn()
            .is_ok()
    };
    if !started {
        return Err(Error::Join("Daemon konnte nicht gestartet werden".into()));
    }
    // Longer than the daemon is willing to wait for its port.
    for _ in 0..200 {
        if client.get(base).send().await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(Error::Join("Daemon-Start Timeout".into()))
}
