use crate::{
    capture::{Ring, clip, mark, redact, redact_url},
    cdp::Cdp,
    control::Arbitration,
    extension::{Bootstrap, ExtensionHub, Message},
    proxy::AgentAccess,
    *,
};
use base64::Engine;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::{Mutex, broadcast},
};

#[derive(Clone)]
pub struct SessionConfig {
    pub session_id: String,
    pub root: PathBuf,
    pub extension_dir: PathBuf,
    pub chromium_path: PathBuf,
    pub urls: Vec<String>,
}
impl SessionConfig {
    pub fn new(root: PathBuf, extension_dir: PathBuf) -> Self {
        Self {
            session_id: uuid::Uuid::now_v7().to_string(),
            root,
            extension_dir,
            chromium_path: "/usr/bin/chromium".into(),
            urls: vec!["about:blank".into()],
        }
    }
}
struct Scope(String);
impl Drop for Scope {
    fn drop(&mut self) {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}
pub struct Browser {
    config: SessionConfig,
    cdp: Cdp,
    endpoint: String,
    child: Mutex<Child>,
    scope: Scope,
    pub extension: ExtensionHub,
    pub control: Arbitration,
    events: broadcast::Sender<BrowserEvent>,
    capture_lock: Mutex<()>,
    remembered: std::sync::Arc<Mutex<Vec<String>>>,
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    page_sessions: Mutex<std::collections::HashMap<String, String>>,
}
impl Browser {
    pub async fn start(config: SessionConfig, extension: ExtensionHub) -> Result<Self> {
        if config.session_id != extension.bootstrap.session_id {
            return Err(Error::Invalid("bootstrap/session mismatch".into()));
        }
        private_dir(&config.root).await?;
        let profile = config.root.join("profile");
        private_dir(&profile).await?;
        let runtime = config
            .root
            .join(format!("extension-{}", uuid::Uuid::now_v7()));
        copy_tree(&config.extension_dir, &runtime)?;
        tokio::fs::write(
            runtime.join("bootstrap.json"),
            serde_json::to_vec(&extension.bootstrap)?,
        )
        .await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(
                runtime.join("bootstrap.json"),
                std::fs::Permissions::from_mode(0o600),
            )
            .await?;
        }
        let active = profile.join("DevToolsActivePort");
        if active.exists() {
            tokio::fs::remove_file(&active).await?;
        }
        let scope = Scope(format!("zuruf-browser-{}.scope", uuid::Uuid::now_v7()));
        let mut command = Command::new("systemd-run");
        command
            .args([
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                &format!("--unit={}", scope.0),
                "-p",
                "MemoryMax=3G",
                "-p",
                "MemorySwapMax=1G",
                "--",
            ])
            .arg(&config.chromium_path)
            .args([
                "--ozone-platform=wayland",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-session-crashed-bubble",
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=0",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--load-extension={}", runtime.display()))
            .args(&config.urls)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let endpoint = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if child.try_wait()?.is_some() {
                    return Err(Error::Closed);
                }
                if let Ok(text) = tokio::fs::read_to_string(&active).await {
                    let mut lines = text.lines();
                    if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                        let port: u16 = port
                            .parse()
                            .map_err(|_| Error::Invalid("debugging port".into()))?;
                        if path.starts_with("/devtools/browser/") {
                            return Ok(format!("ws://127.0.0.1:{port}{path}"));
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| Error::Timeout("Chromium startup"))??;
        let cdp = Cdp::connect(&endpoint).await?;
        tracing::info!(session_id=%config.session_id,"managed Chromium started");
        let (events, _) = broadcast::channel(256);
        let remembered = std::sync::Arc::new(Mutex::new(config.urls.clone()));
        let stopping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stopping.clone();
        let mut disconnected = cdp.subscribe();
        let notify = events.clone();
        tokio::spawn(async move {
            while let Ok(event) = disconnected.recv().await {
                if event["method"] == "Zuruf.disconnected"
                    && !stop_flag.load(std::sync::atomic::Ordering::Relaxed)
                {
                    let _ = notify.send(BrowserEvent::Crashed);
                    break;
                }
            }
        });
        let mut ext_events = extension.subscribe();
        let notify = events.clone();
        let id = config.session_id.clone();
        let urls = remembered.clone();
        tokio::spawn(async move {
            while let Ok(event) = ext_events.recv().await {
                if let Message::Tabs { tabs } = &event.message {
                    *urls.lock().await = tabs.iter().map(|t| t.url.clone()).collect();
                    let _ = notify.send(BrowserEvent::TabsChanged(
                        tabs.iter()
                            .map(|t| BrowserTab {
                                id: t.target_id.clone(),
                                session_id: id.clone(),
                                url: redact_url(&t.url),
                                title: t.title.clone(),
                                active: t.active,
                                scriptable: t.scriptable,
                            })
                            .collect(),
                    ));
                }
                let _ = notify.send(BrowserEvent::Extension(Box::new(event)));
            }
        });
        Ok(Self {
            config,
            cdp,
            endpoint,
            child: Mutex::new(child),
            scope,
            extension,
            control: Arbitration::default(),
            events,
            capture_lock: Mutex::new(()),
            remembered,
            stopping,
            page_sessions: Mutex::new(std::collections::HashMap::new()),
        })
    }
    pub fn bootstrap(port: u16, session_id: String) -> Bootstrap {
        Bootstrap {
            port,
            session_id,
            token: format!("{}{}", uuid::Uuid::now_v7(), uuid::Uuid::now_v7()),
            protocol_version: 1,
        }
    }
    pub fn events(&self) -> broadcast::Receiver<BrowserEvent> {
        self.events.subscribe()
    }
    pub async fn list_tabs(&self) -> Result<Vec<BrowserTab>> {
        let targets = self.cdp.call("Target.getTargets", json!({}), None).await?;
        let registry = self.extension.tabs().await;
        Ok(targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| {
                t["type"] == "page"
                    && !t["url"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("chrome-extension://")
            })
            .map(|t| {
                let id = t["targetId"].as_str().unwrap_or_default();
                let ext = registry.iter().find(|x| x.target_id == id);
                BrowserTab {
                    id: id.into(),
                    session_id: self.config.session_id.clone(),
                    url: redact_url(t["url"].as_str().unwrap_or_default()),
                    title: t["title"].as_str().unwrap_or_default().into(),
                    active: ext.is_some_and(|t| t.active),
                    scriptable: ext.is_some_and(|t| t.scriptable),
                }
            })
            .collect())
    }
    async fn page_session(&self, target: &str) -> Result<String> {
        let mut sessions = self.page_sessions.lock().await;
        if let Some(session) = sessions.get(target) {
            return Ok(session.clone());
        }
        let session = self.cdp.attach(target).await?;
        sessions.insert(target.into(), session.clone());
        Ok(session)
    }
    /// Lab/acceptance aid. Uses a real browser zoom, not CSS zoom.
    pub async fn set_zoom(&self, target: &str, zoom: f64) -> Result<()> {
        if !zoom.is_finite() || !(0.25..=5.0).contains(&zoom) {
            return Err(Error::Invalid("zoom range".into()));
        }
        let tab = self
            .extension
            .tabs()
            .await
            .into_iter()
            .find(|t| t.target_id == target)
            .ok_or(Error::Closed)?;
        let targets = self.cdp.call("Target.getTargets", json!({}), None).await?;
        let worker = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| {
                t["type"] == "service_worker"
                    && t["url"].as_str().is_some_and(|u| u.ends_with("/worker.js"))
            })
            .and_then(|t| t["targetId"].as_str())
            .ok_or(Error::Closed)?;
        let session = self.cdp.attach(worker).await?;
        let result=self.cdp.call("Runtime.evaluate",json!({"expression":format!("chrome.tabs.setZoom({}, {})",tab.tab_id,zoom),"awaitPromise":true}),Some(&session)).await;
        let _ = self
            .cdp
            .call(
                "Target.detachFromTarget",
                json!({"sessionId":session}),
                None,
            )
            .await;
        let value = result?;
        if value.get("exceptionDetails").is_some() {
            return Err(Error::Cdp(value));
        }
        Ok(())
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
    pub async fn open_tab(&self, url: &str) -> Result<String> {
        let value = self
            .cdp
            .call("Target.createTarget", json!({"url":url}), None)
            .await?;
        Ok(value["targetId"].as_str().unwrap_or_default().into())
    }
    pub async fn set_review_mode(&self, on: bool, bindings: Vec<Binding>) -> Result<()> {
        self.extension.configure(on, bindings).await
    }
    pub async fn push_bindings(&self, bindings: Vec<Binding>) -> Result<()> {
        self.extension.push_bindings(bindings).await
    }
    pub async fn grant(&self, agent: String, lease: Duration) -> Result<()> {
        self.control.grant(agent, lease).await?;
        let control = self.control.clone();
        let events = self.events.clone();
        let extension = self.extension.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if control.expire().await {
                    let state = control.snapshot().await;
                    let _ = events.send(BrowserEvent::ControlChanged(state.clone()));
                    let _ = extension.send(Message::Control(state), None, None).await;
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
        let _ = self
            .extension
            .send(Message::Control(state), None, None)
            .await;
    }
    pub async fn takeover(&self) -> Result<()> {
        tracing::info!(session_id=%self.config.session_id,"human takeover");
        self.control.begin_takeover().await;
        self.control_event().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let tabs = match self.list_tabs().await {
            Ok(tabs) => tabs,
            Err(error) => {
                self.control.finish_takeover().await;
                self.control_event().await;
                return Err(error);
            }
        };
        for tab in tabs {
            if let Ok(session) = self.cdp.attach(&tab.id).await {
                let _ = self
                    .cdp
                    .call("Page.stopLoading", json!({}), Some(&session))
                    .await;
                let _ = self
                    .cdp
                    .call(
                        "Target.detachFromTarget",
                        json!({"sessionId":session}),
                        None,
                    )
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
        proxy::serve(
            ws,
            &self.endpoint,
            auth,
            authorization,
            self.control.clone(),
        )
        .await
    }
    /// Validate the source document epoch at both ends of a hotkey capture.
    pub async fn capture_request(
        &self,
        request: &extension::Envelope,
        ring: Ring,
    ) -> Result<CaptureResult> {
        self.capture_request_stage(request, Some(ring)).await
    }
    pub async fn capture_request_raw(
        &self,
        request: &extension::Envelope,
    ) -> Result<CaptureResult> {
        self.capture_request_stage(request, None).await
    }
    async fn capture_request_stage(
        &self,
        request: &extension::Envelope,
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
        if self.extension.document(tab, frame).await.as_ref() != Some(document) {
            return Err(Error::Denied("stale document"));
        }
        let target = self
            .extension
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
        let pointer_frame = self.extension.pointer(tab).await.0;
        let pointer_document = self.extension.document(tab, pointer_frame).await;
        // Keyboard focus can remain in the top frame while the pointer is in a child.
        let context = (pointer_frame == frame).then(|| (*cursor, dom.clone()));
        let result = self
            .capture_stage(&target, kind, *rect, ring, context)
            .await?;
        if self.extension.document(tab, pointer_frame).await != pointer_document {
            return Err(Error::Denied("pointer document changed during capture"));
        }
        if self.extension.document(tab, frame).await.as_ref() != Some(document) {
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
        let tab = self
            .list_tabs()
            .await?
            .into_iter()
            .find(|t| t.id == target)
            .ok_or(Error::Denied("unknown tab"))?;
        let ext = self
            .extension
            .tabs()
            .await
            .into_iter()
            .find(|t| t.target_id == target && t.scriptable)
            .ok_or(Error::Denied("unscriptable tab"))?;
        let (frame, live_cursor) = self.extension.pointer(ext.tab_id).await;
        let cursor = context.as_ref().map(|c| c.0).unwrap_or(live_cursor);
        let mut dom = if let Some((_, dom)) = context {
            dom
        } else if matches!(kind, CaptureKind::Dom) {
            let response = self
                .extension
                .request(Message::DomRequest, ext.tab_id, Some(frame))
                .await?;
            match response.message {
                Message::DomResult { dom, .. } => dom,
                _ => None,
            }
        } else {
            None
        };
        self.extension
            .request(Message::Hide, ext.tab_id, None)
            .await?;
        let session = self.page_session(target).await?;
        tracing::info!(
            "ZURUF_CAPTURE {}",
            json!({"prepareMs":stage_started.elapsed().as_secs_f64()*1000.})
        );
        let result = self
            .capture_inner(&session, tab, kind, rect, cursor, &mut dom, ring)
            .await;
        let _ = self
            .extension
            .send(Message::Show, Some(ext.tab_id), None)
            .await;
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
            // Frame/shadow paths are supplied by the extension; unresolved paths fail closed.
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
            let zoom = self
                .extension
                .tabs()
                .await
                .into_iter()
                .find(|t| t.target_id == tab.id)
                .map(|t| t.zoom)
                .ok_or(Error::Closed)?;
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
        let _ = self.cdp.call("Browser.close", json!({}), None).await;
        let mut child = self.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_err()
        {
            Command::new("systemctl")
                .args(["--user", "stop", &self.scope.0])
                .status()
                .await?;
            child.wait().await?;
        }
        Ok(())
    }
    /// Restart uses remembered page URLs; cookies remain in the same private profile.
    pub async fn restart(&self) -> Result<Self> {
        let mut config = self.config.clone();
        config.urls = self.remembered.lock().await.clone();
        self.stop().await?;
        Self::start(config, self.extension.clone()).await
    }
    /// A duplicate is isolated, with no authentication data copied.
    pub async fn duplicate_config(&self, root: PathBuf) -> Result<SessionConfig> {
        let mut c = SessionConfig::new(root, self.config.extension_dir.clone());
        c.urls = self.list_tabs().await?.into_iter().map(|t| t.url).collect();
        Ok(c)
    }
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
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Denied("extension symlink"));
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}
