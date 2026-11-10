//! A deliberately small local fixture server, not the production daemon.
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use zuruf_browser::{
    extension::{ExtensionHub, Message},
    proxy::AgentAccess,
    *,
};

#[tokio::main]
async fn main() -> Result<()> {
    let automated = std::env::args().any(|a| a == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join(".spike/browser-lab");
    tokio::fs::create_dir_all(&out).await?;
    let mut config = SessionConfig::new(
        out.join(if automated {
            format!("session-{}", uuid::Uuid::now_v7())
        } else {
            "session".into()
        }),
        root.join("apps/extension/dist"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let base = format!("http://127.0.0.1:{port}");
    config.urls = vec![format!("{base}/login")];
    let hub = ExtensionHub::new(Browser::bootstrap(port, config.session_id.clone()));
    let session = Arc::new(RwLock::new(None::<Arc<Browser>>));
    let holder = session.clone();
    let ext = hub.clone();
    let server_root = root.clone();
    let token = format!("{}{}", uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let server_token = token.clone();
    let extension_id = tokio::fs::read_to_string(root.join("apps/extension/extension-id.txt"))
        .await?
        .trim()
        .to_owned();
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let holder = holder.clone();
            let ext = ext.clone();
            let root = server_root.clone();
            let token = server_token.clone();
            let extension_id = extension_id.clone();
            tokio::spawn(async move {
                if let Err(error) = serve(stream, holder, ext, root, token, extension_id).await {
                    eprintln!("lab connection: {error}");
                }
            });
        }
    });
    let browser = Arc::new(Browser::start(config, hub.clone()).await?);
    *session.write().await = Some(browser.clone());
    let bindings = default_bindings();
    browser.set_review_mode(true, bindings).await?;
    let capture_holder = session.clone();
    let output = out.clone();
    let mut events = hub.subscribe();
    let capture_task = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let Message::Capture { ref kind, .. } = event.message {
                let Some(captures) = capture_holder.read().await.clone() else {
                    continue;
                };
                let received = Instant::now();
                let kind = match kind.as_str() {
                    "viewport" => CaptureKind::Viewport,
                    "region" => CaptureKind::Region,
                    "dom" => CaptureKind::Dom,
                    _ => continue,
                };
                match captures
                    .capture_request(&event, capture::Ring::default())
                    .await
                {
                    Ok(result) => {
                        let id = event.request_id;
                        let raw = output.join(format!("{id}-raw.png"));
                        let marked = output.join(format!("{id}-marked.png"));
                        if let Err(e) = tokio::fs::write(raw, &result.raw_png).await {
                            eprintln!("{e}");
                            continue;
                        }
                        let raw_ms = received.elapsed().as_secs_f64() * 1000.0;
                        let keypress_raw_ms = now_ms().saturating_sub(event.timestamp);
                        if let Err(e) = tokio::fs::write(marked, &result.marked_png).await {
                            eprintln!("{e}");
                            continue;
                        }
                        let metadata = serde_json::json!({"requestId":id,"kind":kind,"dom":result.dom,"viewport":result.viewport,"scroll":result.scroll,"cursor":result.cursor,"rect":result.rect,"rawMs":raw_ms,"markedMs":received.elapsed().as_secs_f64()*1000.0,"keypressRawMs":keypress_raw_ms,"keypressMarkedMs":now_ms().saturating_sub(event.timestamp)});
                        let _ = tokio::fs::write(
                            output.join(format!("{id}.json")),
                            metadata.to_string(),
                        )
                        .await;
                        let _ = captures
                            .extension
                            .send(Message::Saved, event.tab_id, None)
                            .await;
                    }
                    Err(error) => {
                        eprintln!("capture: {error}");
                        let _ = captures
                            .extension
                            .send(
                                Message::Failed {
                                    reason: error.to_string(),
                                },
                                event.tab_id,
                                None,
                            )
                            .await;
                    }
                }
            }
        }
    });
    println!(
        "Zuruf-Labor: W/S/D im Reviewfenster. Ausgabe: {}. Strg+C beendet.",
        out.display()
    );
    let result = if automated {
        browser
            .grant("lab".into(), Duration::from_secs(600))
            .await?;
        let status = tokio::process::Command::new("node")
            .arg(root.join("tests/browser/integration.mjs"))
            .env(
                "ZURUF_ENDPOINT",
                format!(
                    "ws://127.0.0.1:{port}/cdp/{}",
                    browser.extension.bootstrap.session_id
                ),
            )
            .env("ZURUF_TOKEN", &token)
            .env("ZURUF_BASE", &base)
            .env("ZURUF_OUTPUT", &out)
            .kill_on_drop(true)
            .status()
            .await;
        status.map_err(Error::Io).and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(Error::Invalid(format!("integration exited {s}")))
            }
        })
    } else {
        tokio::signal::ctrl_c().await.map_err(Error::Io)
    };
    capture_task.abort();
    if let Some(current) = session.write().await.take() {
        current.stop().await?;
    }
    server.abort();
    result
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
fn default_bindings() -> Vec<Binding> {
    [
        ("KeyW", "capture.viewport"),
        ("KeyS", "capture.region"),
        ("KeyD", "capture.dom"),
        ("KeyE", "comment.text"),
        ("KeyR", "speech.toggle"),
        ("Escape", "action.cancel"),
        ("KeyP", "panel.toggle"),
    ]
    .into_iter()
    .map(|(code, action)| Binding {
        action: action.into(),
        scope: "page".into(),
        chord: Some(Chord {
            mods: vec![],
            code: code.into(),
        }),
    })
    .chain(std::iter::once(Binding {
        action: "batch.dispatch".into(),
        scope: "page".into(),
        chord: Some(Chord {
            mods: vec!["shift".into()],
            code: "KeyN".into(),
        }),
    }))
    .collect()
}
#[allow(clippy::result_large_err)] // tungstenite handshake callback fixes this error type.
async fn serve(
    mut stream: TcpStream,
    holder: Arc<RwLock<Option<Arc<Browser>>>>,
    hub: ExtensionHub,
    root: PathBuf,
    token: String,
    extension_id: String,
) -> Result<()> {
    let mut buffer = [0u8; 16384];
    let size = loop {
        let size = stream.peek(&mut buffer).await?;
        if size == 0 {
            return Ok(());
        }
        if buffer[..size].windows(4).any(|x| x == b"\r\n\r\n") {
            break size;
        }
        if size == buffer.len() {
            return Err(Error::Invalid("headers too large".into()));
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    };
    let header = String::from_utf8_lossy(&buffer[..size]);
    let path = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned();
    if header.to_ascii_lowercase().contains("upgrade: websocket") {
        let mut origin = String::new();
        let mut auth = None;
        let ws =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                origin = request
                    .headers()
                    .get("origin")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .into();
                auth = request
                    .headers()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                Ok(response)
            })
            .await?;
        if path == "/ext" {
            return hub.serve(ws, &origin, &extension_id).await;
        }
        let browser = holder.read().await.clone().ok_or(Error::Closed)?;
        if path != format!("/cdp/{}", browser.extension.bootstrap.session_id) {
            return Err(Error::Denied("session path"));
        }
        return browser
            .serve_agent(ws, AgentAccess::new("lab".into(), token), auth.as_deref())
            .await;
    }
    let mut consumed = vec![0; size];
    stream.read_exact(&mut consumed).await?;
    let browser = holder.read().await.clone();
    let authorized = header.lines().any(|l| {
        l == format!("Authorization: Bearer {token}")
            || l == format!("authorization: Bearer {token}")
    });
    let mut extra = "";
    let body = if path.starts_with("/test/") {
        if !authorized {
            return Err(Error::Denied("lab token"));
        }
        let b = browser.ok_or(Error::Closed)?;
        if path.starts_with("/test/metrics?") {
            let query = path.split('?').nth(1).unwrap_or_default();
            let mut dpr = 1.0;
            let mut zoom = 1.0;
            for pair in query.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    let value: f64 = value.parse().map_err(|_| Error::Invalid("metric".into()))?;
                    match key {
                        "dpr" => dpr = value,
                        "zoom" => zoom = value,
                        _ => {}
                    }
                }
            }
            let tab = b
                .list_tabs()
                .await?
                .into_iter()
                .find(|t| t.active)
                .ok_or(Error::Closed)?;
            b.emulate_dpr(&tab.id, dpr).await?;
            b.set_zoom(&tab.id, zoom).await?;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        match path.as_str() {
            "/test/takeover" => {
                b.takeover().await?;
            }
            "/test/release" => {
                b.release().await;
                b.grant("lab".into(), Duration::from_secs(600)).await?;
            }
            "/test/isolated" => {
                let mut config = b
                    .duplicate_config(root.join(format!(
                        ".spike/browser-lab/isolated-{}",
                        uuid::Uuid::now_v7()
                    )))
                    .await?;
                // Laboratory profile swap keeps the endpoint identity; real duplication
                // gets its own Bootstrap/session/token from the daemon.
                config.session_id = b.extension.bootstrap.session_id.clone();
                config.urls = vec![format!("http://127.0.0.1:{}/", b.extension.bootstrap.port)];
                b.stop().await?;
                let isolated = Arc::new(Browser::start(config, b.extension.clone()).await?);
                isolated
                    .grant("lab".into(), Duration::from_secs(600))
                    .await?;
                *holder.write().await = Some(isolated);
            }
            "/test/restart" => {
                let restarted = Arc::new(b.restart().await?);
                restarted.set_review_mode(true, default_bindings()).await?;
                restarted
                    .grant("lab".into(), Duration::from_secs(600))
                    .await?;
                *holder.write().await = Some(restarted);
            }
            _ => {}
        }
        b"ok".to_vec()
    } else {
        if path == "/login" {
            extra = "Set-Cookie: zuruf_fixture=synthetic; Path=/; Max-Age=3600; SameSite=Lax\r\n";
        }
        tokio::fs::read(
            root.join("tests/browser/fixture")
                .join(if path == "/frame.html" {
                    "frame.html"
                } else {
                    "index.html"
                }),
        )
        .await?
    };
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",body.len()).as_bytes()).await?;
    stream.write_all(&body).await?;
    Ok(())
}
