//! One browser session: a Zuruf shell window with its own profile, driven over CDP.
use crate::{
    bridge::{Envelope, Message, PageBridge},
    capture::{Ring, clip, mark, redact, redact_url},
    cdp::Cdp,
    control::Arbitration,
    proxy::{AgentAccess, TabOpener},
    *,
};
use base64::Engine;
use serde_json::json;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, broadcast},
};

#[derive(Clone)]
pub struct SessionConfig {
    pub session_id: String,
    /// Private directory of the session; the browser profile lives below it.
    pub root: PathBuf,
    /// The `zuruf-shell` executable.
    pub shell_path: PathBuf,
    /// Pages to open as tabs.
    pub urls: Vec<String>,
    /// The app's own page, docked beside the tabs. Never a tab and never visible to agents.
    pub panel_url: Option<String>,
    /// The app's theme for the window's own controls: `system`, `dark` or `light`.
    pub theme: String,
    /// Extra switches for the shell, for example `--no-sandbox` inside a container.
    pub shell_args: Vec<String>,
}
impl SessionConfig {
    pub fn new(root: PathBuf) -> Self {
        Self {
            session_id: uuid::Uuid::now_v7().to_string(),
            root,
            shell_path: default_shell_path(),
            urls: vec!["about:blank".into()],
            panel_url: None,
            theme: "system".into(),
            shell_args: std::env::var("ZURUF_SHELL_ARGS")
                .map(|args| args.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
        }
    }
}

/// Where the shell is: `ZURUF_SHELL`, next to this executable, in `../lib/zuruf`, or one
/// directory up.
pub fn default_shell_path() -> PathBuf {
    let name = format!("zuruf-shell{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = std::env::var_os("ZURUF_SHELL") {
        return path.into();
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_owned));
    if let Some(directory) = beside {
        let candidates = [
            directory.join(&name),
            // Installed layout: bin/zuruf and lib/zuruf/zuruf-shell.
            directory.join("../lib/zuruf").join(&name),
            // Tests and examples run from a subdirectory of the build directory.
            directory.join("..").join(&name),
        ];
        if let Some(found) = candidates.into_iter().find(|path| path.exists()) {
            // A plain path reads better in diagnostics and in the process list.
            return found.canonicalize().unwrap_or(found);
        }
    }
    name.into()
}

/// A systemd scope that limits the shell's memory; stopped when the session ends.
struct Scope(Option<String>);
impl Scope {
    /// Only where a systemd user session exists; everywhere else the shell runs unconfined.
    fn new() -> Self {
        let available = cfg!(target_os = "linux")
            && std::env::var_os("XDG_RUNTIME_DIR")
                .is_some_and(|runtime| Path::new(&runtime).join("systemd/private").exists());
        Self(available.then(|| format!("zuruf-browser-{}.scope", uuid::Uuid::now_v7())))
    }
    fn stop(&self) {
        if let Some(unit) = &self.0 {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "stop", unit])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The parts needed to open a tab in the session's window, shared with agent connections.
#[derive(Clone)]
struct Window {
    stdin: Arc<Mutex<ChildStdin>>,
    page: PageBridge,
    opening: Arc<Mutex<()>>,
}
impl Window {
    async fn command(&self, command: serde_json::Value) -> Result<()> {
        send_command(&self.stdin, command).await
    }
    /// Ask the shell for a new tab and wait until the browser reports its page.
    async fn open_tab(&self, url: &str) -> Result<String> {
        // One at a time, so the tab that appears is the one we asked for.
        let _turn = self.opening.lock().await;
        let known: HashSet<String> = self
            .page
            .tabs()
            .await
            .into_iter()
            .map(|tab| tab.target_id)
            .collect();
        self.command(json!({"command": "open", "url": url})).await?;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let opened = self
                .page
                .tabs()
                .await
                .into_iter()
                .find(|tab| !known.contains(&tab.target_id));
            if let Some(tab) = opened {
                return Ok(tab.target_id);
            }
        }
        Err(Error::Timeout("new tab"))
    }
}

pub struct Browser {
    config: SessionConfig,
    cdp: Cdp,
    endpoint: String,
    child: Mutex<Child>,
    pid: Option<u32>,
    scope: Scope,
    window: Window,
    private: HashSet<String>,
    pub page: PageBridge,
    pub control: Arbitration,
    events: broadcast::Sender<BrowserEvent>,
    capture_lock: Mutex<()>,
    remembered: Arc<Mutex<Vec<String>>>,
    stopping: Arc<std::sync::atomic::AtomicBool>,
}
impl Browser {
    pub async fn start(config: SessionConfig) -> Result<Self> {
        private_dir(&config.root).await?;
        let profile = config.root.join("profile");
        private_dir(&profile).await?;
        let port = free_port()?;
        let scope = Scope::new();
        let mut command = shell_command(&config.shell_path, &scope, "3G");
        command
            .arg(format!("--zuruf-profile={}", profile.display()))
            .arg(format!("--zuruf-theme={}", config.theme))
            .arg(format!("--zuruf-port={port}"));
        if let Some(panel) = &config.panel_url {
            command.arg(format!("--zuruf-panel={panel}"));
        }
        for url in &config.urls {
            command.arg(format!("--zuruf-url={url}"));
        }
        command.args(&config.shell_args);
        let (mut child, stdin, closed) = spawn_shell(command, &config.shell_path, &config.root)?;
        let pid = child.id();
        let endpoint = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if child.try_wait()?.is_some() {
                    return Err(Error::Closed);
                }
                if let Ok(endpoint) = debugger_endpoint(port).await {
                    return Ok(endpoint);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| Error::Timeout("browser startup"))??;
        let cdp = Cdp::connect(&endpoint).await?;
        let private = match &config.panel_url {
            Some(panel) => panel_targets(&cdp, panel).await?,
            None => HashSet::new(),
        };
        let page =
            PageBridge::start(cdp.clone(), config.session_id.clone(), private.clone()).await?;
        // The first tabs exist already; give their attach events a moment to arrive.
        for _ in 0..120 {
            if !page.tabs().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        tracing::info!(session_id = %config.session_id, "browser session started");

        let (events, _) = broadcast::channel(256);
        let remembered = Arc::new(Mutex::new(config.urls.clone()));
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stopping.clone();
        let mut disconnected = cdp.subscribe();
        let notify = events.clone();
        tokio::spawn(async move {
            loop {
                match disconnected.recv().await {
                    Ok(event) if event["method"] == "Zuruf.disconnected" => {
                        if !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                            // A window the user closed said so before it went away.
                            let ended = if closed.load(std::sync::atomic::Ordering::Relaxed) {
                                BrowserEvent::Closed
                            } else {
                                BrowserEvent::Crashed
                            };
                            let _ = notify.send(ended);
                        }
                        break;
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let mut page_events = page.subscribe();
        let notify = events.clone();
        let id = config.session_id.clone();
        let urls = remembered.clone();
        tokio::spawn(async move {
            loop {
                let event = match page_events.recv().await {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if let Message::Tabs { tabs } = &event.message {
                    if !tabs.is_empty() {
                        *urls.lock().await = tabs.iter().map(|tab| tab.url.clone()).collect();
                    }
                    let _ = notify.send(BrowserEvent::TabsChanged(
                        tabs.iter().map(|tab| browser_tab(&id, tab)).collect(),
                    ));
                }
                let _ = notify.send(BrowserEvent::Page(Box::new(event)));
            }
        });
        let window = Window {
            stdin: Arc::new(Mutex::new(stdin)),
            page: page.clone(),
            opening: Arc::default(),
        };
        Ok(Self {
            config,
            cdp,
            endpoint,
            child: Mutex::new(child),
            pid,
            scope,
            window,
            private,
            page,
            control: Arbitration::default(),
            events,
            capture_lock: Mutex::new(()),
            remembered,
            stopping,
        })
    }
    pub fn events(&self) -> broadcast::Receiver<BrowserEvent> {
        self.events.subscribe()
    }
    pub async fn list_tabs(&self) -> Result<Vec<BrowserTab>> {
        Ok(self
            .page
            .tabs()
            .await
            .iter()
            .map(|tab| browser_tab(&self.config.session_id, tab))
            .collect())
    }
    async fn page_session(&self, target: &str) -> Result<String> {
        self.page
            .page_session(target)
            .await
            .ok_or(Error::Denied("unknown tab"))
    }
    /// Lab-only DPR emulation; never passed through to an agent browser context.
    pub async fn emulate_dpr(&self, target: &str, dpr: f64) -> Result<()> {
        let session = self.page_session(target).await?;
        self.cdp
            .call(
                "Emulation.setDeviceMetricsOverride",
                json!({"width":1000,"height":800,"deviceScaleFactor":dpr,"mobile":false}),
                Some(&session),
            )
            .await?;
        Ok(())
    }
    /// Open a page in a new tab of the session's window. Returns its target id.
    pub async fn open_tab(&self, url: &str) -> Result<String> {
        self.window.open_tab(url).await
    }
    /// Process id of the shell, which is also the owner of its window.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
    /// Bring the session's window to the front.
    pub async fn focus(&self) -> Result<()> {
        self.window.command(json!({"command": "focus"})).await
    }
    /// Show or hide the app panel beside the page; toggles when `visible` is `None`.
    pub async fn show_panel(&self, visible: Option<bool>) -> Result<()> {
        self.window
            .command(json!({"command": "panel", "visible": visible}))
            .await
    }
    /// Change the theme of the window's own controls: `system`, `dark` or `light`.
    pub async fn set_theme(&self, theme: &str) -> Result<()> {
        self.window
            .command(json!({"command": "theme", "mode": theme}))
            .await
    }
    pub async fn set_review_mode(&self, on: bool, bindings: Vec<Binding>) -> Result<()> {
        self.page.configure(on, bindings).await;
        Ok(())
    }
    pub async fn push_bindings(&self, bindings: Vec<Binding>) -> Result<()> {
        self.page.push_bindings(bindings).await;
        Ok(())
    }
    /// Show in every page whether speech is being recorded.
    pub async fn set_recording(&self, recording: bool) {
        self.page
            .send(Message::Status { recording }, None, None)
            .await;
    }
    pub async fn grant(&self, agent: String, lease: Duration) -> Result<()> {
        self.control.grant(agent, lease).await?;
        let control = self.control.clone();
        let events = self.events.clone();
        let page = self.page.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if control.expire().await {
                    let state = control.snapshot().await;
                    let _ = events.send(BrowserEvent::ControlChanged(state.clone()));
                    page.send(Message::Control(state), None, None).await;
                    break;
                }
                if control.snapshot().await.owner != "agent" {
                    break;
                }
            }
        });
        self.control_event().await;
        Ok(())
    }
    pub async fn renew(&self, agent: &str, lease: Duration) -> Result<()> {
        self.control.renew(agent, lease).await?;
        self.control_event().await;
        Ok(())
    }
    pub async fn release(&self) {
        self.control.release().await;
        self.control_event().await;
    }
    async fn control_event(&self) {
        let state = self.control.snapshot().await;
        let _ = self
            .events
            .send(BrowserEvent::ControlChanged(state.clone()));
        self.page.send(Message::Control(state), None, None).await;
    }
    pub async fn takeover(&self) -> Result<()> {
        tracing::info!(session_id=%self.config.session_id,"human takeover");
        self.control.begin_takeover().await;
        self.control_event().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        for tab in self.page.tabs().await {
            if let Some(session) = self.page.page_session(&tab.target_id).await {
                let _ = self
                    .cdp
                    .call("Page.stopLoading", json!({}), Some(&session))
                    .await;
            }
        }
        self.control.finish_takeover().await;
        self.control_event().await;
        Ok(())
    }
    pub async fn serve_agent<S>(
        &self,
        ws: tokio_tungstenite::WebSocketStream<S>,
        auth: AgentAccess,
        authorization: Option<&str>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let window = self.window.clone();
        let opener: TabOpener = Arc::new(move |url: String| {
            let window = window.clone();
            Box::pin(async move { window.open_tab(&url).await })
        });
        proxy::serve(
            ws,
            &self.endpoint,
            auth,
            authorization,
            self.control.clone(),
            self.private.clone(),
            Some(opener),
        )
        .await
    }
    /// Validate the source document epoch at both ends of a hotkey capture.
    pub async fn capture_request(&self, request: &Envelope, ring: Ring) -> Result<CaptureResult> {
        self.capture_request_stage(request, Some(ring)).await
    }
    pub async fn capture_request_raw(&self, request: &Envelope) -> Result<CaptureResult> {
        self.capture_request_stage(request, None).await
    }
    async fn capture_request_stage(
        &self,
        request: &Envelope,
        ring: Option<Ring>,
    ) -> Result<CaptureResult> {
        let Message::Capture {
            kind,
            rect,
            cursor,
            dom,
        } = &request.message
        else {
            return Err(Error::Invalid("expected capture request".into()));
        };
        let tab = request.tab_id.ok_or(Error::Denied("missing tab"))?;
        let frame = request.frame_id.ok_or(Error::Denied("missing frame"))?;
        let document = request
            .document_id
            .as_ref()
            .ok_or(Error::Denied("missing document epoch"))?;
        if self.page.document(tab, frame).await.as_ref() != Some(document) {
            return Err(Error::Denied("stale document"));
        }
        let target = self
            .page
            .tabs()
            .await
            .into_iter()
            .find(|t| t.tab_id == tab)
            .ok_or(Error::Denied("unknown tab"))?
            .target_id;
        let kind = match kind.as_str() {
            "viewport" => CaptureKind::Viewport,
            "region" => CaptureKind::Region,
            "dom" => CaptureKind::Dom,
            _ => return Err(Error::Invalid("capture kind".into())),
        };
        let pointer_frame = self.page.pointer(tab).await.0;
        let pointer_document = self.page.document(tab, pointer_frame).await;
        // Keyboard focus can remain in the top frame while the pointer is in a child.
        let context = (pointer_frame == frame).then(|| (*cursor, dom.as_deref().cloned()));
        let result = self
            .capture_stage(&target, kind, *rect, ring, context)
            .await?;
        if self.page.document(tab, pointer_frame).await != pointer_document {
            return Err(Error::Denied("pointer document changed during capture"));
        }
        if self.page.document(tab, frame).await.as_ref() != Some(document) {
            return Err(Error::Denied("document changed during capture"));
        }
        Ok(result)
    }
    pub async fn capture(
        &self,
        target: &str,
        kind: CaptureKind,
        rect: Option<Rect>,
        ring: Ring,
    ) -> Result<CaptureResult> {
        self.capture_stage(target, kind, rect, Some(ring), None)
            .await
    }
    pub async fn capture_raw(
        &self,
        target: &str,
        kind: CaptureKind,
        rect: Option<Rect>,
    ) -> Result<CaptureResult> {
        self.capture_stage(target, kind, rect, None, None).await
    }
    async fn capture_stage(
        &self,
        target: &str,
        kind: CaptureKind,
        rect: Option<Rect>,
        ring: Option<Ring>,
        context: Option<(Point, Option<DomCapture>)>,
    ) -> Result<CaptureResult> {
        let stage_started = std::time::Instant::now();
        let _lock = self.capture_lock.lock().await;
        let page = self
            .page
            .tabs()
            .await
            .into_iter()
            .find(|t| t.target_id == target)
            .ok_or(Error::Denied("unknown tab"))?;
        if !page.scriptable {
            return Err(Error::Denied("unscriptable tab"));
        }
        let tab = browser_tab(&self.config.session_id, &page);
        let (frame, live_cursor) = self.page.pointer(page.tab_id).await;
        let cursor = context.as_ref().map(|c| c.0).unwrap_or(live_cursor);
        let mut dom = if let Some((_, dom)) = context {
            dom
        } else if matches!(kind, CaptureKind::Dom) {
            self.page.dom(page.tab_id, frame).await?.0
        } else {
            None
        };
        self.page.hide(page.tab_id).await?;
        let session = self.page_session(target).await?;
        tracing::info!(
            "ZURUF_CAPTURE {}",
            json!({"prepareMs":stage_started.elapsed().as_secs_f64()*1000.})
        );
        let result = self
            .capture_inner(&session, tab, kind, rect, cursor, &mut dom, ring)
            .await;
        self.page.send(Message::Show, Some(page.tab_id), None).await;
        result
    }
    #[allow(clippy::too_many_arguments)] // Capture context stays explicit across CDP steps.
    async fn capture_inner(
        &self,
        session: &str,
        tab: BrowserTab,
        kind: CaptureKind,
        rect: Option<Rect>,
        cursor: Point,
        dom: &mut Option<DomCapture>,
        ring: Option<Ring>,
    ) -> Result<CaptureResult> {
        let stage_started = std::time::Instant::now();
        let selectors = std::env::var("ZURUF_MASK_SELECTORS").unwrap_or_default();
        let selector = format!(
            "input[type=password], [data-zuruf-private]{}{}",
            if selectors.is_empty() { "" } else { "," },
            selectors
        );
        let expression = format!(
            "({{width:innerWidth,height:innerHeight,dpr:devicePixelRatio,x:scrollX,y:scrollY,title:document.title,url:location.href,masks:Array.from(document.querySelectorAll({})).map(e=>{{const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}})}})",
            serde_json::to_string(&selector)?
        );
        // Read geometry and masks in one page snapshot; independent CDP reads overlap.
        let (epoch, metrics, eval) = tokio::try_join!(
            self.cdp.call("Page.getFrameTree", json!({}), Some(session)),
            async {
                if matches!(kind, CaptureKind::Region) {
                    self.cdp
                        .call("Page.getLayoutMetrics", json!({}), Some(session))
                        .await
                } else {
                    Ok(json!({}))
                }
            },
            self.cdp.call(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true}),
                Some(session)
            ),
        )?;
        if eval.get("exceptionDetails").is_some() {
            return Err(Error::Invalid("invalid mask selector".into()));
        }
        let m = &eval["result"]["value"];
        let mut rectangles: Vec<Rect> = serde_json::from_value(m["masks"].clone())?;
        let number = |key: &str| m[key].as_f64().unwrap_or(0.0);
        let viewport = Viewport {
            width: number("width"),
            height: number("height"),
            device_scale_factor: number("dpr"),
        };
        if let Some(d) = dom {
            // AX lookup uses an isolated execution world and an element reference.
            // Frame/shadow paths are supplied by the page agent; unresolved paths fail closed.
            if !d.frame_path.iter().any(|p| p.starts_with("unresolved:")) {
                let paths = serde_json::to_string(&d.frame_path)?;
                let expression = format!(
                    "(()=>{{let doc=document,x={},y={};for(const path of {}){{const f=doc.querySelector(path);if(!f?.contentDocument)return null;const r=f.getBoundingClientRect();x-=r.x+f.clientLeft;y-=r.y+f.clientTop;doc=f.contentDocument;}}let e=doc.elementFromPoint(x,y);while(e?.shadowRoot){{let n=e.shadowRoot.elementFromPoint(x,y);if(!n||n===e)break;e=n;}}return e;}})()",
                    cursor.x, cursor.y, paths
                );
                let object = self
                    .cdp
                    .call(
                        "Runtime.evaluate",
                        json!({"expression":expression}),
                        Some(session),
                    )
                    .await?;
                if let Some(id) = object["result"]["objectId"].as_str() {
                    let ax = self
                        .cdp
                        .call(
                            "Accessibility.getPartialAXTree",
                            json!({"objectId":id,"fetchRelatives":false}),
                            Some(session),
                        )
                        .await?;
                    if let Some(node) = ax["nodes"].as_array().and_then(|a| a.first()) {
                        d.role = node["role"]["value"].as_str().map(str::to_owned);
                        d.name = node["name"]["value"].as_str().map(str::to_owned);
                        d.locators.retain(|l| l.kind != "role");
                        if let (Some(role), Some(name)) = (&d.role, &d.name) {
                            d.locators.push(LocatorCandidate {
                                kind: "role".into(),
                                value: role.clone(),
                                playwright: Some(format!(
                                    "getByRole({}, {{ name: {}, exact: true }})",
                                    serde_json::to_string(role)?,
                                    serde_json::to_string(name)?
                                )),
                                stable: true,
                                reason: Some("CDP Accessibility validated".into()),
                            });
                        }
                    }
                    let _ = self
                        .cdp
                        .call(
                            "Runtime.releaseObject",
                            json!({"objectId":id}),
                            Some(session),
                        )
                        .await;
                }
            } else {
                d.locators.retain(|l| l.kind != "role");
                d.role = None;
                d.name = None;
            }
            redact(d);
            for locator in &mut d.locators {
                if let Some(expression) = &mut locator.playwright {
                    let mut prefix = String::new();
                    for path in &d.frame_path {
                        prefix
                            .push_str(&format!("frameLocator({}).", serde_json::to_string(path)?));
                    }
                    if !d.shadow_path.is_empty() && locator.kind == "css" {
                        *expression = format!(
                            "locator({})",
                            serde_json::to_string(&format!(
                                "{} {}",
                                d.shadow_path.join(" "),
                                locator.value
                            ))?
                        );
                    }
                    *expression = format!("{prefix}{expression}");
                }
            }
        }
        let mut params = json!({"format":"png","captureBeyondViewport":false,"fromSurface":true,"optimizeForSpeed":true});
        if matches!(kind, CaptureKind::Region) {
            let r = rect.ok_or_else(|| Error::Invalid("region requires rect".into()))?;
            // Browser zoom: CSS pixels to device-independent pixels.
            let zoom = metrics["cssVisualViewport"]["zoom"]
                .as_f64()
                .filter(|zoom| *zoom > 0.0)
                .unwrap_or(1.0);
            let offset = Point {
                x: metrics["cssVisualViewport"]["pageX"]
                    .as_f64()
                    .unwrap_or(0.0),
                y: metrics["cssVisualViewport"]["pageY"]
                    .as_f64()
                    .unwrap_or(0.0),
            };
            let clip = clip(r, offset, zoom)?;
            params["clip"] = serde_json::to_value(clip)?;
            params["clip"]["scale"] = json!(1);
        }
        let metadata_ms = stage_started.elapsed().as_secs_f64() * 1000.;
        let shot_started = std::time::Instant::now();
        let shot = self
            .cdp
            .call("Page.captureScreenshot", params, Some(session))
            .await?;
        let screenshot_ms = shot_started.elapsed().as_secs_f64() * 1000.;
        let post_started = std::time::Instant::now();
        let after = self
            .cdp
            .call("Page.getFrameTree", json!({}), Some(session))
            .await?;
        if epoch != after {
            return Err(Error::Denied("navigation during capture"));
        }
        let raw = base64::engine::general_purpose::STANDARD
            .decode(
                shot["data"]
                    .as_str()
                    .ok_or_else(|| Error::Invalid("missing PNG".into()))?,
            )
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if let Some(d) = dom.as_mut()
            && rectangles.iter().any(|r| {
                r.x < d.bounding_box.x + d.bounding_box.width
                    && r.x + r.width > d.bounding_box.x
                    && r.y < d.bounding_box.y + d.bounding_box.height
                    && r.y + r.height > d.bounding_box.y
            })
        {
            d.text = None;
            d.name = None;
            d.attributes.clear();
            d.locators.clear();
            d.parent = None;
            d.children.clear();
        }
        let image_viewport = if let Some(rect) = rect {
            for mask in &mut rectangles {
                mask.x -= rect.x;
                mask.y -= rect.y;
            }
            Point {
                x: rect.width,
                y: rect.height,
            }
        } else {
            Point {
                x: viewport.width,
                y: viewport.height,
            }
        };
        let raw = crate::capture::mask(&raw, image_viewport, &rectangles)?;
        let marked = if let Some(ring) = ring {
            if matches!(kind, CaptureKind::Region) {
                raw.clone()
            } else {
                mark(
                    &raw,
                    Point {
                        x: viewport.width,
                        y: viewport.height,
                    },
                    cursor,
                    ring,
                    dom.as_ref().map(|d| d.bounding_box),
                )?
            }
        } else {
            vec![]
        };
        tracing::info!(
            "ZURUF_CAPTURE {}",
            json!({"metadataMs":metadata_ms,"screenshotMs":screenshot_ms,"postprocessMs":post_started.elapsed().as_secs_f64()*1000.})
        );
        Ok(CaptureResult {
            raw_png: raw,
            marked_png: marked,
            url: redact_url(m["url"].as_str().unwrap_or_default()),
            title: m["title"].as_str().unwrap_or_default().into(),
            viewport,
            scroll: Point {
                x: number("x"),
                y: number("y"),
            },
            cursor,
            rect,
            dom: dom.clone(),
            tab,
        })
    }
    pub async fn stop(&self) -> Result<()> {
        self.stopping
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.window.command(json!({"command": "quit"})).await;
        let mut child = self.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_err()
        {
            self.scope.stop();
            let _ = child.start_kill();
            child.wait().await?;
        }
        Ok(())
    }
    /// Restart uses remembered page URLs; cookies remain in the same private profile.
    pub async fn restart(&self) -> Result<Self> {
        let mut config = self.config.clone();
        config.urls = self.remembered.lock().await.clone();
        self.stop().await?;
        Self::start(config).await
    }
    /// A duplicate is isolated, with no authentication data copied.
    pub async fn duplicate_config(&self, root: PathBuf) -> Result<SessionConfig> {
        let mut config = SessionConfig::new(root);
        config.shell_path = self.config.shell_path.clone();
        config.shell_args = self.config.shell_args.clone();
        config.panel_url = self.config.panel_url.clone();
        config.theme = self.config.theme.clone();
        config.urls = self.list_tabs().await?.into_iter().map(|t| t.url).collect();
        Ok(config)
    }
}

/// The shell's command line up to its own switches, inside a memory-limited scope if possible.
fn shell_command(shell: &Path, scope: &Scope, memory: &str) -> Command {
    match &scope.0 {
        Some(unit) => {
            let mut command = Command::new("systemd-run");
            command.args([
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                &format!("--unit={unit}"),
                "-p",
                &format!("MemoryMax={memory}"),
                "-p",
                "MemorySwapMax=1G",
                "--",
            ]);
            command.arg(shell);
            command
        }
        None => Command::new(shell),
    }
}

/// Set once the shell reports that its window was closed, by the user or on request.
type ClosedFlag = Arc<std::sync::atomic::AtomicBool>;

/// Start a shell with its command channel open. Chromium's own log goes to `shell.log`.
fn spawn_shell(
    mut command: Command,
    shell: &Path,
    directory: &Path,
) -> Result<(Child, ChildStdin, ClosedFlag)> {
    let log = std::fs::File::create(directory.join("shell.log"))?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log)
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        std::io::Error::new(error.kind(), format!("{}: {error}", shell.display()))
    })?;
    let stdin = child.stdin.take().ok_or(Error::Closed)?;
    let closed = ClosedFlag::default();
    if let Some(stdout) = child.stdout.take() {
        let closed = closed.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let event = serde_json::from_str::<serde_json::Value>(&line).unwrap_or_default();
                if event["event"] == "closed" {
                    closed.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                tracing::debug!(line, "shell");
            }
        });
    }
    Ok((child, stdin, closed))
}

/// One command for a shell: a line of JSON on its stdin.
async fn send_command(stdin: &Mutex<ChildStdin>, command: serde_json::Value) -> Result<()> {
    let mut line = command.to_string();
    line.push('\n');
    let mut stdin = stdin.lock().await;
    stdin.write_all(line.as_bytes()).await?;
    stdin.flush().await?;
    Ok(())
}

/// The app in a window of its own: shown while no browser session carries it as a panel.
/// It has no tabs and no debugging endpoint.
pub struct AppWindow {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pid: Option<u32>,
    scope: Scope,
}
impl AppWindow {
    /// `directory` holds the window's small private profile; `url` is the app's page.
    pub async fn start(
        shell: &Path,
        directory: &Path,
        url: &str,
        theme: &str,
        args: &[String],
    ) -> Result<Self> {
        private_dir(directory).await?;
        let profile = directory.join("profile");
        private_dir(&profile).await?;
        let scope = Scope::new();
        let mut command = shell_command(shell, &scope, "1G");
        command
            .arg(format!("--zuruf-profile={}", profile.display()))
            .arg(format!("--zuruf-panel={url}"))
            .arg(format!("--zuruf-theme={theme}"))
            .arg("--zuruf-app-only")
            .args(args);
        let (child, stdin, _closed) = spawn_shell(command, shell, directory)?;
        Ok(Self {
            pid: child.id(),
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            scope,
        })
    }
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
    /// False once the user has closed the window.
    pub async fn alive(&self) -> bool {
        matches!(self.child.lock().await.try_wait(), Ok(None))
    }
    pub async fn focus(&self) -> Result<()> {
        send_command(&self.stdin, json!({"command": "focus"})).await
    }
    pub async fn set_theme(&self, theme: &str) -> Result<()> {
        send_command(&self.stdin, json!({"command": "theme", "mode": theme})).await
    }
    pub async fn stop(&self) {
        let _ = send_command(&self.stdin, json!({"command": "quit"})).await;
        let mut child = self.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .is_err()
        {
            self.scope.stop();
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }
}

fn browser_tab(session: &str, tab: &crate::bridge::PageTab) -> BrowserTab {
    BrowserTab {
        id: tab.target_id.clone(),
        session_id: session.into(),
        url: redact_url(&tab.url),
        title: tab.title.clone(),
        active: tab.active,
        scriptable: tab.scriptable,
    }
}

/// A loopback port that is free right now. The shell binds it a moment later.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// Ask the shell's DevTools endpoint for its browser-level WebSocket address.
async fn debugger_endpoint(port: u16) -> Result<String> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let request = format!(
        "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    let mut chunk = [0u8; 2048];
    // The body is one small JSON object; stop as soon as it parses.
    for _ in 0..64 {
        let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut chunk))
            .await
            .map_err(|_| Error::Timeout("DevTools endpoint"))??;
        response.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&response);
        let body = text.split_once("\r\n\r\n").map(|(_, body)| body);
        if let Some(value) =
            body.and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
            && let Some(address) = value["webSocketDebuggerUrl"].as_str()
        {
            return Ok(address.to_owned());
        }
        if read == 0 {
            break;
        }
    }
    Err(Error::Invalid("DevTools endpoint did not answer".into()))
}

/// The targets showing the app's own page. They are identified once, right after start.
async fn panel_targets(cdp: &Cdp, panel_url: &str) -> Result<HashSet<String>> {
    let address = panel_url.split(['#', '?']).next().unwrap_or(panel_url);
    for _ in 0..300 {
        let targets = cdp.call("Target.getTargets", json!({}), None).await?;
        let found: HashSet<String> = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|info| {
                info["type"] == "page"
                    && info["url"]
                        .as_str()
                        .is_some_and(|url| url.starts_with(address))
            })
            .filter_map(|info| info["targetId"].as_str().map(str::to_owned))
            .collect();
        if !found.is_empty() {
            return Ok(found);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(Error::Timeout("app panel"))
}

async fn private_dir(path: &Path) -> Result<()> {
    if tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(Error::Denied("symlink session directory"));
    }
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}
