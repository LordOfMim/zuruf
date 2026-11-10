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
    Status,
    Action {
        action_id: String,
    },
    Open,
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
    let paths = Paths::from_env()?;
    let port = std::env::var("ZURUF_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(47600);
    let base = format!("http://127.0.0.1:{port}");
    match args.command {
        Command::Daemon { seed_demo } => {
            if std::env::var("ZURUF_FAKE_ADAPTERS").as_deref() != Ok("1") {
                return Err(Error::Api(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "Real adapters are not wired yet. Set ZURUF_FAKE_ADAPTERS=1.".into(),
                ));
            }
            let daemon = Daemon::fake(paths).await?;
            if seed_demo {
                daemon.seed_demo().await?;
            }
            daemon.recover().await?;
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                .await
                .map_err(StoreError::from)?;
            tracing::info!(
                port = listener.local_addr().map_err(StoreError::from)?.port(),
                "daemon listening"
            );
            let result = axum::serve(listener, daemon.router())
                .with_graceful_shutdown(async {
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
            let request = match command {
                Command::Status => client.get(format!("{base}/api/v1/state")),
                Command::Action { action_id } => client
                    .post(format!("{base}/api/v1/actions/{action_id}"))
                    .json(&serde_json::json!({})),
                Command::Open => client
                    .post(format!("{base}/api/v1/auth/code"))
                    .json(&serde_json::json!({})),
                Command::Daemon { .. } => return Ok(()),
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
                println!("{base}/#code={code}");
            } else {
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
        }
    }
    Ok(())
}
