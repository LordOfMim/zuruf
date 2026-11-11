//! End-to-end check of a browser session against a real shell window.
//!
//! Needs the built shell and a display, so it is ignored by default:
//!
//! ```sh
//! ZURUF_SHELL=target/debug/zuruf-shell cargo test -p zuruf-browser --test shell -- --ignored
//! ```
//!
//! Everything an agent does here goes through the same filtered proxy as in production.
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use zuruf_browser::{
    Binding, Browser, BrowserEvent, CaptureKind, Chord, Rect, SessionConfig,
    bridge::{Envelope, Message},
    capture::Ring,
    proxy::AgentAccess,
};

fn fixture_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/browser/fixture")
}

/// Serves the fixture pages. Returns the origin, e.g. `http://127.0.0.1:41234`.
async fn serve_fixture() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut request = vec![0u8; 4096];
                let read = stream.read(&mut request).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..read]);
                let path = text.split_whitespace().nth(1).unwrap_or("/");
                let name = path.split('?').next().unwrap_or("/").trim_start_matches('/');
                let name = if name.is_empty() { "index.html" } else { name };
                let response = match tokio::fs::read(fixture_directory().join(name)).await {
                    Ok(body) if !name.contains("..") => {
                        let mut head = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        head.extend(body);
                        head
                    }
                    _ => b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                        .to_vec(),
                };
                let _ = stream.write_all(&response).await;
            });
        }
    });
    origin
}

/// An agent's CDP connection through the session's proxy.
struct Agent {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next: u64,
}
impl Agent {
    async fn connect(browser: std::sync::Arc<Browser>, agent: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}", listener.local_addr().unwrap());
        let access = AgentAccess::new(agent.into(), "secret".into());
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = browser
                .serve_agent(socket, access, Some("Bearer secret"))
                .await;
        });
        let (socket, _) = tokio_tungstenite::connect_async(address).await.unwrap();
        Self { socket, next: 0 }
    }
    /// Send a command and wait for its reply, skipping events.
    async fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Value {
        self.next += 1;
        let id = self.next;
        let command = json!({"id": id, "method": method, "params": params, "sessionId": session});
        self.socket
            .send(command.to_string().into())
            .await
            .unwrap();
        loop {
            let message = tokio::time::timeout(Duration::from_secs(10), self.socket.next())
                .await
                .expect("agent reply in time")
                .expect("agent connection open")
                .unwrap();
            let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            if value["id"] == id {
                return value;
            }
        }
    }
    async fn press(&mut self, session: &str, key: &str, code: &str, virtual_key: u32) {
        for kind in ["rawKeyDown", "keyUp"] {
            let reply = self
                .call(
                    "Input.dispatchKeyEvent",
                    json!({"type": kind, "key": key, "code": code, "windowsVirtualKeyCode": virtual_key}),
                    Some(session),
                )
                .await;
            assert!(reply.get("error").is_none(), "{reply}");
        }
    }
}

async fn wait_for<T>(what: &str, mut probe: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..200 {
        if let Some(value) = probe().await {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn next_page_message(
    events: &mut tokio::sync::broadcast::Receiver<BrowserEvent>,
    wanted: impl Fn(&Message) -> bool,
) -> Envelope {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("page message in time")
            .expect("browser events open");
        if let BrowserEvent::Page(envelope) = event
            && wanted(&envelope.message)
        {
            return *envelope;
        }
    }
}

fn image_size(png_bytes: &[u8]) -> (u32, u32) {
    let reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    (reader.info().width, reader.info().height)
}

fn binding(action: &str, code: &str) -> Binding {
    Binding {
        action: action.into(),
        scope: "page".into(),
        chord: Some(Chord {
            mods: vec![],
            code: code.into(),
        }),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the built shell (ZURUF_SHELL) and a display"]
async fn session_in_shell_window() {
    let origin = serve_fixture().await;
    let directory = tempfile::tempdir().unwrap();
    let mut config = SessionConfig::new(directory.path().join("session"));
    config.urls = vec![format!("{origin}/index.html")];
    config.panel_url = Some(format!("{origin}/frame.html?panel#code"));
    let browser = std::sync::Arc::new(Browser::start(config).await.expect("shell starts"));
    let mut events = browser.events();

    // The panel is not a tab; the page becomes scriptable once the agent has announced itself.
    let tab = wait_for("a scriptable, visible tab", async || {
        let tabs = browser.list_tabs().await.unwrap();
        assert!(tabs.len() <= 1, "the panel must not be listed: {tabs:?}");
        // The title tells the fixture document apart from the empty one a tab starts with.
        tabs.into_iter()
            .find(|tab| tab.scriptable && tab.active && tab.title == "Zuruf Browser-Spike")
    })
    .await;
    assert!(tab.url.ends_with("/index.html"));

    browser
        .set_review_mode(
            true,
            vec![binding("capture.viewport", "KeyW"), binding("speech.toggle", "KeyR")],
        )
        .await
        .unwrap();

    // An agent sees exactly the page, never the panel.
    browser
        .grant("agent".into(), Duration::from_secs(120))
        .await
        .unwrap();
    let mut agent = Agent::connect(browser.clone(), "agent").await;
    let targets = agent.call("Target.getTargets", json!({}), None).await;
    let pages = targets["result"]["targetInfos"].as_array().unwrap();
    assert_eq!(pages.len(), 1, "{targets}");
    assert_eq!(pages[0]["targetId"], tab.id);
    let attached = agent
        .call(
            "Target.attachToTarget",
            json!({"targetId": tab.id, "flatten": true}),
            None,
        )
        .await;
    let session = attached["result"]["sessionId"].as_str().unwrap().to_owned();

    // A bound key outside a text field becomes an action; the page never sees it.
    agent.press(&session, "r", "KeyR", 82).await;
    let action = next_page_message(&mut events, |m| matches!(m, Message::Action { .. })).await;
    assert!(matches!(&action.message, Message::Action { action } if action == "speech.toggle"));

    // The capture key yields a request bound to the current document.
    agent.press(&session, "w", "KeyW", 87).await;
    let request = next_page_message(&mut events, |m| matches!(m, Message::Capture { .. })).await;
    let shot = browser
        .capture_request(&request, Ring::default())
        .await
        .expect("viewport capture");
    let (width, height) = image_size(&shot.raw_png);
    let scale = shot.viewport.device_scale_factor;
    assert_eq!(width, (shot.viewport.width * scale).round() as u32);
    assert_eq!(height, (shot.viewport.height * scale).round() as u32);
    assert_eq!(image_size(&shot.marked_png), (width, height));

    // A region capture has exactly the requested size.
    let region = browser
        .capture(
            &tab.id,
            CaptureKind::Region,
            Some(Rect {
                x: 20.0,
                y: 30.0,
                width: 200.0,
                height: 120.0,
            }),
            Ring::default(),
        )
        .await
        .expect("region capture");
    assert_eq!(
        image_size(&region.raw_png),
        ((200.0 * scale).round() as u32, (120.0 * scale).round() as u32)
    );

    // The element picker describes what is under the pointer.
    let rect = agent
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const r = document.querySelector('button').getBoundingClientRect(); return {x: r.x + r.width / 2, y: r.y + r.height / 2}; })()",
                "returnByValue": true,
            }),
            Some(&session),
        )
        .await;
    let point = &rect["result"]["result"]["value"];
    agent
        .call(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseMoved", "x": point["x"], "y": point["y"]}),
            Some(&session),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let picked = browser
        .capture(&tab.id, CaptureKind::Dom, None, Ring::default())
        .await
        .expect("element capture");
    let dom = picked.dom.expect("element under the pointer");
    assert_eq!(dom.tag, "button");
    assert!(
        dom.locators.iter().any(|locator| locator.kind == "role"),
        "{dom:?}"
    );

    // A tab an agent creates opens inside the window, as a normal tab.
    let created = agent
        .call(
            "Target.createTarget",
            json!({"url": format!("{origin}/frame.html")}),
            None,
        )
        .await;
    let created = created["result"]["targetId"]
        .as_str()
        .unwrap_or_else(|| panic!("no target: {created}"))
        .to_owned();
    let tabs = wait_for("the agent's tab", async || {
        let tabs = browser.list_tabs().await.unwrap();
        (tabs.len() == 2).then_some(tabs)
    })
    .await;
    assert!(tabs.iter().any(|tab| tab.id == created));

    // The daemon opens tabs the same way.
    let opened = browser.open_tab(&format!("{origin}/popup.html")).await.unwrap();
    assert_eq!(browser.list_tabs().await.unwrap().len(), 3);
    assert_ne!(opened, created);

    // The human always wins: after a takeover the agent's commands are refused.
    browser.takeover().await.unwrap();
    let refused = agent
        .call(
            "Runtime.evaluate",
            json!({"expression": "1 + 1"}),
            Some(&session),
        )
        .await;
    assert!(refused.get("error").is_some(), "{refused}");

    browser.stop().await.expect("shell stops");
}
