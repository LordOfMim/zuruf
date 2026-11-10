//! Adapter boundary for the independently testable runtime crates.
use crate::*;
use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;
use tokio::sync::Mutex as AsyncMutex;
use zuruf_browser::{Browser, SessionConfig, extension::ExtensionHub};

fn convert<T: Serialize, U: DeserializeOwned>(value: T) -> std::result::Result<U, AdapterError> {
    serde_json::from_value(serde_json::to_value(value).map_err(io)?).map_err(io)
}
fn io(e: impl std::fmt::Display) -> AdapterError {
    AdapterError::Io(e.to_string())
}
fn t3_error(e: zuruf_t3::Error) -> AdapterError {
    match e {
        zuruf_t3::Error::Http(_) | zuruf_t3::Error::Io(_) => AdapterError::Ambiguous(
            "T3 transport interrupted; reconcile marker before retry".into(),
        ),
        other => io(other),
    }
}
fn receipt(r: zuruf_t3::ThreadReceipt) -> ThreadReceipt {
    ThreadReceipt {
        thread_id: r.thread_id,
        thread_url: r.link,
        run_id: r.run_id.unwrap_or_default(),
        workspace_path: r.worktree_path,
        branch: r.branch,
    }
}
pub struct T3 {
    inner: AsyncMutex<zuruf_t3::T3Adapter>,
    origin: Option<String>,
}
impl T3 {
    async fn connected(
        &self,
    ) -> std::result::Result<tokio::sync::MutexGuard<'_, zuruf_t3::T3Adapter>, AdapterError> {
        let mut a = self.inner.lock().await;
        if a.connection.state != "connected" {
            if std::env::var("ZURUF_T3_TRANSPORT").as_deref() == Ok("cli") {
                a.connect_cli().await.map_err(t3_error)?;
            } else if let Some(origin) = &self.origin {
                a.connect(origin).await.map_err(t3_error)?;
            } else {
                return Err(AdapterError::Unavailable(
                    "T3_URL fehlt; zuruf t3 pair --origin http://127.0.0.1:<port> ausführen".into(),
                ));
            }
        }
        Ok(a)
    }
}
impl T3Adapter for T3 {
    fn send_message_with_assets(
        &self,
        thread: String,
        request_id: String,
        prompt: String,
        assets: Vec<(String, Vec<u8>)>,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            if assets.is_empty() {
                return self.send_message(thread, request_id, prompt).await;
            }
            if assets.len() > 8 {
                return Err(AdapterError::Rejected(
                    "Maximal acht Bilder pro T3-Nachricht".into(),
                ));
            }
            let mut adapter = self.connected().await?;
            let mut attachments = Vec::new();
            for (name, bytes) in assets {
                attachments.push(
                    adapter
                        .upload_image(&name, "image/png", bytes)
                        .await
                        .map_err(t3_error)?,
                );
            }
            // This verified endpoint has no request-id parameter; never replay an ambiguous send.
            adapter
                .send_attachments(&thread, &prompt, attachments)
                .await
                .map(receipt)
                .map_err(|e| AdapterError::Ambiguous(e.to_string()))
        })
    }

    fn create_thread_with_assets(
        &self,
        r: CreateThreadRequest,
        assets: Vec<(String, Vec<u8>)>,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            if assets.len() > 8 {
                return Err(AdapterError::Rejected(
                    "Maximal acht Bilder pro T3-Thread".into(),
                ));
            }
            let profile = convert(r.profile)?;
            let mut a = self.connected().await?;
            let mut attachments = Vec::new();
            for (name, bytes) in assets {
                attachments.push(
                    a.upload_image(&name, "image/png", bytes)
                        .await
                        .map_err(t3_error)?,
                );
            }
            let result = a
                .create_thread_with_attachments(
                    &r.project.repository_path,
                    &r.title,
                    &r.prompt,
                    &profile,
                    &r.client_request_id,
                    attachments,
                )
                .await
                .map_err(|e| AdapterError::Ambiguous(e.to_string()))?;
            Ok(receipt(result))
        })
    }

    fn discover_capabilities(&self) -> AdapterFuture<'_, T3Connection> {
        Box::pin(async move { convert(&self.connected().await?.connection) })
    }
    fn list_projects(&self) -> AdapterFuture<'_, Vec<T3ProjectRef>> {
        Box::pin(async move {
            convert(
                self.connected()
                    .await?
                    .list_projects()
                    .await
                    .map_err(t3_error)?,
            )
        })
    }
    fn catalog(&self, search: Option<String>) -> AdapterFuture<'_, T3Catalog> {
        Box::pin(async move {
            convert(
                self.connected()
                    .await?
                    .catalog(search.as_deref())
                    .await
                    .map_err(t3_error)?,
            )
        })
    }
    fn create_thread(&self, r: CreateThreadRequest) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            let profile = convert(r.profile)?;
            let result = self
                .connected()
                .await?
                .create_thread(
                    &r.project.repository_path,
                    &r.title,
                    &r.prompt,
                    &profile,
                    &r.client_request_id,
                )
                .await
                .map_err(t3_error)?;
            Ok(receipt(result))
        })
    }
    fn send_message(
        &self,
        thread: String,
        request: String,
        prompt: String,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            Ok(receipt(
                self.connected()
                    .await?
                    .send_message(&thread, &prompt, &request)
                    .await
                    .map_err(t3_error)?,
            ))
        })
    }
    fn thread_status(&self, thread: String, run: String) -> AdapterFuture<'_, RemoteStatus> {
        Box::pin(async move {
            let mut a = self.connected().await?;
            let run = if run.is_empty() {
                let v = a
                    .call("t3_thread_read", json!({"threadId":thread}))
                    .await
                    .map_err(t3_error)?;
                v["thread"]["latestRunId"].as_str().unwrap_or("").to_owned()
            } else {
                run
            };
            let s = a.thread_status(&thread, &run).await.map_err(t3_error)?;
            let status = s.remote_status;
            Ok(RemoteStatus {
                error: s.error,
                run_id: run,
                status,
                last_assistant_message: s.last_message,
                pending_request: s.pending_request,
                approval: s.pending_kind.as_deref() == Some("approval"),
            })
        })
    }
    fn find_threads_by_marker(
        &self,
        project: Project,
        marker: String,
    ) -> AdapterFuture<'_, Vec<ThreadReceipt>> {
        Box::pin(async move {
            let values = self
                .connected()
                .await?
                .find_threads_by_marker(&project.repository_path, &marker)
                .await
                .map_err(t3_error)?;
            values
                .into_iter()
                .map(|v| {
                    Ok(ThreadReceipt {
                        thread_id: v["threadId"]
                            .as_str()
                            .ok_or_else(|| io("missing threadId"))?
                            .into(),
                        thread_url: v["link"].as_str().map(str::to_owned),
                        run_id: v["latestRunId"].as_str().unwrap_or("").into(),
                        workspace_path: v["worktreePath"].as_str().map(str::to_owned),
                        branch: v["branch"].as_str().map(str::to_owned),
                    })
                })
                .collect()
        })
    }
    fn interrupt(&self, thread: String) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.connected()
                .await?
                .interrupt(&thread)
                .await
                .map_err(t3_error)
        })
    }
}

struct SpeechInner {
    backend: Option<Box<dyn zuruf_speech::SpeechAdapter>>,
    boundary: Option<SpeechBoundary>,
    started: String,
    levels: Option<tokio::task::AbortHandle>,
}
pub struct Speech {
    app: std::sync::OnceLock<std::sync::Weak<Inner>>,
    inner: AsyncMutex<SpeechInner>,
    directory: PathBuf,
    pub(crate) settings: std::sync::RwLock<SettingsSpeech>,
}
impl Speech {
    fn handle(
        h: zuruf_speech::SegmentHandle,
        old: SpeechBoundary,
        started: String,
        ended: String,
    ) -> SegmentHandle {
        let bounds = h.segment();
        let origin =
            time::OffsetDateTime::parse(&started, &time::format_description::well_known::Rfc3339)
                .ok();
        let timestamp = |sample| {
            origin.and_then(|origin| {
                (origin + time::Duration::microseconds((sample * 1_000_000 / 16000) as i64))
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
        };
        let started = timestamp(bounds.start_sample).unwrap_or(started);
        let ended = timestamp(bounds.end_sample).unwrap_or(ended);
        SegmentHandle {
            segment: TranscriptSegment {
                id: h.id.clone(),
                batch_id: old.batch_id,
                annotation_id: old.annotation_id,
                started_at: started,
                ended_at: Some(ended),
                state: TranscriptSegmentState::Transcribing,
                original_text: String::new(),
            },
            transcript: Box::pin(async move {
                let s = h.resolve().await;
                match s.state {
                    zuruf_speech::SegmentState::Failed | zuruf_speech::SegmentState::Cancelled => {
                        Err(io(s
                            .error
                            .unwrap_or_else(|| "Transkription abgebrochen".into())))
                    }
                    _ => Ok(s.original_text),
                }
            }),
        }
    }
}
impl SpeechAdapter for Speech {
    fn state(&self) -> AdapterFuture<'_, SpeechState> {
        Box::pin(async move {
            let i = self.inner.lock().await;
            match &i.backend {
                Some(b) => convert(b.state()),
                None => convert(json!({"state":"idle","pendingSegments":0})),
            }
        })
    }
    fn start_epoch(&self, boundary: SpeechBoundary) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            use zuruf_speech::SpeechAdapter;
            let settings = self.settings.read().map_err(io)?.clone();
            let model = settings.model.unwrap_or_else(|| "base".into());
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| io("HOME missing"))?;
            if !home
                .join(format!(".local/share/voxtype/models/ggml-{model}.bin"))
                .exists()
            {
                let detail = format!("Sprachmodell {model} fehlt. Kein automatischer Download.");
                if let Some(app) = self.app.get().and_then(std::sync::Weak::upgrade) {
                    let _ = app.events.send(json!({"type":"notice","payload":{"level":"error","code":"speech.model_missing","detail":detail}}));
                }
                return Err(AdapterError::Unavailable(detail));
            }
            let mut i = self.inner.lock().await;
            if i.backend.is_some() {
                return Err(AdapterError::Rejected("Sprache läuft bereits".into()));
            }
            let mut b = zuruf_speech::Continuous::new(
                zuruf_speech::Voxtype {
                    executable: settings.voxtype_path.clone().into(),
                    directory: self.directory.clone(),
                    model: Some(model.clone()),
                    language: Some(settings.language.unwrap_or_else(|| "de".into())),
                    engine: None,
                    keep_audio: settings.keep_audio,
                },
                1,
            );
            let levels = b.levels();
            let continuous = if settings.backend == SettingsSpeechBackend::Daemon {
                Err(zuruf_speech::Error::Backend(
                    "Daemon-Backend ausgewählt".into(),
                ))
            } else {
                b.start(boundary.batch_id.clone()).await
            };
            let backend: Box<dyn zuruf_speech::SpeechAdapter> = match continuous {
                Ok(()) => Box::new(b),
                Err(error) => {
                    if let Some(app) = self.app.get().and_then(std::sync::Weak::upgrade) {
                        let detail = format!(
                            "Daemon-Sprachbackend: verzögerte Segmentgrenzen, keine Sprachüberschreibung. {error}"
                        );
                        let _ = app.events.send(json!({"type":"notice","payload":{"level":"warning","code":"speech.daemon_fallback","detail":detail}}));
                    }
                    let mut fallback = zuruf_speech::Daemon::new(
                        settings.voxtype_path.into(),
                        self.directory.clone(),
                        Some(model),
                    );
                    fallback
                        .start(boundary.batch_id.clone())
                        .await
                        .map_err(io)?;
                    Box::new(fallback)
                }
            };
            if let Some(weak) = self.app.get().cloned() {
                i.levels = Some(
                    tokio::spawn(async move {
                        let mut levels = levels;
                        while levels.changed().await.is_ok() {
                            let Some(app) = weak.upgrade() else {
                                break;
                            };
                            let _ = app.events.send(
                                json!({"type":"speech.level","payload":{"level":*levels.borrow()}}),
                            );
                        }
                    })
                    .abort_handle(),
                );
            }
            i.started = boundary.at.clone();
            i.boundary = Some(boundary);
            i.backend = Some(backend);
            Ok(())
        })
    }
    fn mark(&self, boundary: SpeechBoundary) -> AdapterFuture<'_, Vec<SegmentHandle>> {
        Box::pin(async move {
            let mut i = self.inner.lock().await;
            let old = i.boundary.clone().ok_or_else(|| io("not recording"))?;
            let h = i
                .backend
                .as_mut()
                .ok_or_else(|| io("not recording"))?
                .mark(
                    boundary.annotation_id.clone(),
                    Some(boundary.batch_id.clone()),
                )
                .await
                .map_err(io)?;
            let started = i.started.clone();
            i.boundary = Some(boundary.clone());
            Ok(vec![Self::handle(h, old, started, boundary.at)])
        })
    }
    fn stop(&self) -> AdapterFuture<'_, Vec<SegmentHandle>> {
        Box::pin(async move {
            let boundary = self.inner.lock().await.boundary.clone();
            let Some(mut boundary) = boundary else {
                return Ok(vec![]);
            };
            boundary.at = now();
            let handles = self.mark(boundary).await?;
            let mut i = self.inner.lock().await;
            if let Some(mut b) = i.backend.take() {
                if let Some(levels) = i.levels.take() {
                    levels.abort();
                }
                // The final handle must finish before cancellation invalidates workers.
                b.finish().await.map_err(io)?;
            }
            i.boundary = None;
            Ok(handles)
        })
    }
    fn cancel(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            let mut i = self.inner.lock().await;
            if let Some(mut b) = i.backend.take() {
                b.cancel().await.map_err(io)?;
            }
            i.boundary = None;
            Ok(())
        })
    }
}
pub struct Hotkeys(AsyncMutex<zuruf_hypr::Hyprland>);
impl DesktopHotkeys for Hotkeys {
    fn apply(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            let issues = self
                .0
                .lock()
                .await
                .apply(convert(bindings)?)
                .await
                .map_err(io)?;
            if issues.is_empty() {
                Ok(())
            } else {
                Err(AdapterError::Rejected(
                    serde_json::to_string(&issues).map_err(io)?,
                ))
            }
        })
    }
    fn check_conflicts(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, Vec<BindingIssue>> {
        Box::pin(async move {
            let output = tokio::process::Command::new("hyprctl")
                .args(["binds", "-j"])
                .output()
                .await
                .map_err(io)?;
            let existing: Vec<zuruf_hypr::ExistingBind> =
                serde_json::from_slice(&output.stdout).map_err(io)?;
            let (_, issues) = zuruf_hypr::prepare(
                &std::env::current_exe().map_err(io)?,
                &convert::<_, Vec<zuruf_hypr::Binding>>(bindings)?,
                &existing,
            );
            convert(issues)
        })
    }
    fn clear(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move { self.0.lock().await.shutdown().await.map_err(io) })
    }
}

tokio::task_local! {
    static CAPTURE_ENVELOPE: std::cell::RefCell<Option<zuruf_browser::extension::Envelope>>;
}
pub struct Browsers {
    pub sessions: AsyncMutex<BTreeMap<String, Arc<Browser>>>,
    pub hubs: AsyncMutex<BTreeMap<String, ExtensionHub>>,
    bindings: AsyncMutex<Vec<Binding>>,
    review: AsyncMutex<bool>,
    paths: Paths,
    port: u16,
    app: std::sync::OnceLock<std::sync::Weak<Inner>>,
    tabs: AsyncMutex<BTreeMap<String, String>>,
}
impl Browsers {
    async fn get(&self, id: &str) -> std::result::Result<Arc<Browser>, AdapterError> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| AdapterError::Unavailable("Browser nicht gestartet".into()))
    }
    async fn tab(
        &self,
        tab: zuruf_browser::BrowserTab,
    ) -> std::result::Result<BrowserTab, AdapterError> {
        let key = format!("{}:{}", tab.session_id, tab.id);
        let mut ids = self.tabs.lock().await;
        let mapped = ids.entry(key).or_insert_with(id).clone();
        let bytes = serde_json::to_vec(&*ids).map_err(io)?;
        let path = self.paths.state.join("tab-ids.json");
        tokio::task::spawn_blocking(move || zuruf_store::atomic_write(&path, &bytes))
            .await
            .map_err(io)?
            .map_err(io)?;
        let mut result: BrowserTab = convert(tab)?;
        result.id = mapped;
        Ok(result)
    }
    async fn listen(
        self: Arc<Self>,
        session: String,
        mut rx: broadcast::Receiver<zuruf_browser::BrowserEvent>,
    ) {
        while let Ok(event) = rx.recv().await {
            let Some(inner) = self.app.get().and_then(std::sync::Weak::upgrade) else {
                break;
            };
            let app = Daemon(inner);
            match event {
                zuruf_browser::BrowserEvent::TabsChanged(tabs) => {
                    let mut mapped = vec![];
                    for t in tabs {
                        if let Ok(t) = self.tab(t).await {
                            mapped.push(t);
                        }
                    }
                    let sid = session.clone();
                    let _ = app
                        .mutate(move |s, _| {
                            if let Some(session) = s.sessions.iter_mut().find(|x| x.id == sid) {
                                session.tabs = mapped;
                            }
                            Ok(())
                        })
                        .await;
                }
                zuruf_browser::BrowserEvent::Crashed => {
                    let sid = session.clone();
                    let _ = app
                        .mutate(move |s, _| {
                            if let Some(session) = s.sessions.iter_mut().find(|x| x.id == sid) {
                                session.state = BrowserSessionState::Crashed;
                            }
                            Ok(())
                        })
                        .await;
                }
                zuruf_browser::BrowserEvent::ControlChanged(control) => {
                    if let Ok(control) = convert::<_, ControlState>(control) {
                        let sid = session.clone();
                        let _ = app
                            .mutate(move |s, _| {
                                if let Some(session) = s.sessions.iter_mut().find(|x| x.id == sid) {
                                    let mut control = control;
                                    if control.owner == ControlStateOwner::Agent
                                        && control.agent_id == session.control.agent_id
                                    {
                                        control.lease_expires_at =
                                            session.control.lease_expires_at.clone();
                                    }
                                    session.control = control;
                                }
                                Ok(())
                            })
                            .await;
                    }
                }
                zuruf_browser::BrowserEvent::Extension(envelope) => {
                    use zuruf_browser::extension::Message;
                    let action = match &envelope.message {
                        Message::Action { action } => Some(action.clone()),
                        Message::Capture { kind, .. } => Some(format!("capture.{kind}")),
                        _ => None,
                    };
                    if let Some(action) = action {
                        let app = app.clone();
                        let sid = session.clone();
                        tokio::spawn(CAPTURE_ENVELOPE.scope(
                            std::cell::RefCell::new(
                                if matches!(envelope.message, Message::Capture { .. }) {
                                    Some(*envelope)
                                } else {
                                    None
                                },
                            ),
                            async move {
                                let _ = app
                                    .mutate(move |s, _| {
                                        s.status.active_session_id = Some(sid);
                                        Ok(())
                                    })
                                    .await;
                                if let Err(e) = app.action(&action).await {
                                    tracing::warn!(error=%e,"extension action failed");
                                }
                            },
                        ));
                    }
                }
            }
        }
    }
}
#[derive(Clone)]
pub struct BrowserAdapter(Arc<Browsers>);
impl std::ops::Deref for BrowserAdapter {
    type Target = Browsers;
    fn deref(&self) -> &Browsers {
        &self.0
    }
}
impl BrowserController for BrowserAdapter {
    fn start_session(&self, mut s: BrowserSession) -> AdapterFuture<'_, BrowserSession> {
        Box::pin(async move {
            if self.sessions.lock().await.contains_key(&s.id) {
                return Ok(s);
            }
            let hub = ExtensionHub::new(Browser::bootstrap(self.port, s.id.clone()));
            self.hubs.lock().await.insert(s.id.clone(), hub.clone());
            let mut config = SessionConfig::new(
                self.paths.data.join("sessions").join(&s.id),
                self.paths.data.join("extension-bundle"),
            );
            config.session_id = s.id.clone();
            config.urls = s.tabs.iter().map(|t| t.url.clone()).collect();
            if config.urls.is_empty() {
                let start = if let Some(inner) = self.app.get().and_then(std::sync::Weak::upgrade) {
                    Daemon(inner)
                        .snapshot()
                        .await
                        .map_err(io)?
                        .projects
                        .into_iter()
                        .find(|p| p.id == s.project_id)
                        .map(|p| p.base_url)
                } else {
                    None
                };
                config.urls.push(
                    start
                        .filter(|url| !url.is_empty())
                        .unwrap_or_else(|| "about:blank".into()),
                );
            }
            let browser = Arc::new(Browser::start(config, hub).await.map_err(io)?);
            browser
                .push_bindings(convert(self.bindings.lock().await.clone())?)
                .await
                .map_err(io)?;
            browser
                .set_review_mode(
                    *self.review.lock().await,
                    convert(self.bindings.lock().await.clone())?,
                )
                .await
                .map_err(io)?;
            let rx = browser.events();
            self.sessions.lock().await.insert(s.id.clone(), browser);
            s.state = BrowserSessionState::Running;
            s.control = ControlState {
                owner: ControlStateOwner::None,
                agent_id: None,
                lease_expires_at: None,
                settling: false,
            };
            s.tabs = self.list_tabs(s.id.clone()).await?;
            tokio::spawn(self.0.clone().listen(s.id.clone(), rx));
            Ok(s)
        })
    }
    fn stop_session(&self, session: Id) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            if let Some(b) = self.sessions.lock().await.remove(&session) {
                b.stop().await.map_err(io)?;
            }
            self.hubs.lock().await.remove(&session);
            if let Some(inner) = self.app.get().and_then(std::sync::Weak::upgrade)
                && let Some(runtime) = inner.real.get()
                && let Some(access) = runtime.access.lock().await.remove(&session)
            {
                access.revoke();
            }
            Ok(())
        })
    }
    fn duplicate_session(
        &self,
        mut source: BrowserSession,
        new_id: Id,
    ) -> AdapterFuture<'_, BrowserSession> {
        Box::pin(async move {
            source.id = new_id;
            source.tabs.clear();
            source.created_at = now();
            source.state = BrowserSessionState::Stopped;
            source.agent_access = false;
            self.start_session(source).await
        })
    }
    fn open_tab(&self, session: Id, url: String) -> AdapterFuture<'_, BrowserTab> {
        Box::pin(async move {
            let b = self.get(&session).await?;
            let target = b.open_tab(&url).await.map_err(io)?;
            let tab = b
                .list_tabs()
                .await
                .map_err(io)?
                .into_iter()
                .find(|t| t.id == target)
                .ok_or_else(|| io("tab missing"))?;
            self.tab(tab).await
        })
    }
    fn list_tabs(&self, session: Id) -> AdapterFuture<'_, Vec<BrowserTab>> {
        Box::pin(async move {
            let mut result = vec![];
            for t in self.get(&session).await?.list_tabs().await.map_err(io)? {
                result.push(self.tab(t).await?);
            }
            Ok(result)
        })
    }
    fn capture(&self, r: CaptureRequest) -> AdapterFuture<'_, CaptureResult> {
        Box::pin(async move {
            let b = self.get(&r.session_id).await?;
            let mut ring = zuruf_browser::capture::Ring::default();
            if let Some(inner) = self.app.get().and_then(std::sync::Weak::upgrade) {
                let settings = Daemon(inner).snapshot().await.map_err(io)?.settings.capture;
                ring.radius = settings.ring_radius;
                if let Some(hex) = settings.ring_color.strip_prefix('#')
                    && hex.len() == 6
                    && let Ok(rgb) = u32::from_str_radix(hex, 16)
                {
                    ring.color = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
                }
            }
            // Keep each hotkey's document epoch and geometry attached to its own task.
            let envelope = CAPTURE_ENVELOPE
                .try_with(|slot| slot.borrow_mut().take())
                .ok()
                .flatten();
            let result = if let Some(envelope) = envelope {
                b.capture_request(&envelope, ring).await.map_err(io)?
            } else {
                let tabs = b.list_tabs().await.map_err(io)?;
                let target_id = if let Some(tab_id) = &r.tab_id {
                    self.tabs
                        .lock()
                        .await
                        .iter()
                        .find(|(key, value)| {
                            key.starts_with(&format!("{}:", r.session_id)) && *value == tab_id
                        })
                        .map(|(key, _)| key[r.session_id.len() + 1..].to_owned())
                } else {
                    None
                };
                let target = tabs
                    .iter()
                    .find(|t| target_id.as_ref().map_or(t.active, |id| &t.id == id))
                    .or(tabs.first())
                    .ok_or_else(|| io("no tab"))?;
                let kind = convert(r.kind)?;
                b.capture(&target.id, kind, r.rect.map(convert).transpose()?, ring)
                    .await
                    .map_err(io)?
            };
            let dimensions = |bytes: &[u8]| -> std::result::Result<(u32, u32), AdapterError> {
                let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
                let reader = decoder.read_info().map_err(io)?;
                Ok((reader.info().width, reader.info().height))
            };
            let mut images = vec![];
            for (kind, bytes) in [
                (AssetKind::Screenshot, result.raw_png),
                (AssetKind::ScreenshotMarked, result.marked_png),
            ] {
                let (width, height) = dimensions(&bytes)?;
                images.push(CaptureImage {
                    kind,
                    bytes,
                    width,
                    height,
                });
            }
            Ok(CaptureResult {
                images,
                tab: self.tab(result.tab).await?,
                viewport: Some(convert(result.viewport)?),
                scroll: Some(convert(result.scroll)?),
                cursor: Some(convert(result.cursor)?),
                rect: result.rect.map(convert).transpose()?,
                dom: result.dom.map(convert).transpose()?,
            })
        })
    }
    fn set_review_mode(&self, on: bool) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            *self.review.lock().await = on;
            let bindings: Vec<zuruf_browser::Binding> =
                convert(self.bindings.lock().await.clone())?;
            let browsers: Vec<_> = self.sessions.lock().await.values().cloned().collect();
            for b in browsers {
                b.set_review_mode(on, bindings.clone()).await.map_err(io)?;
            }
            Ok(())
        })
    }
    fn push_bindings(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            *self.bindings.lock().await = bindings.clone();
            let bindings: Vec<zuruf_browser::Binding> = convert(bindings)?;
            let browsers: Vec<_> = self.sessions.lock().await.values().cloned().collect();
            for b in browsers {
                b.push_bindings(bindings.clone()).await.map_err(io)?;
            }
            Ok(())
        })
    }
    fn control(&self, session: Id, r: ControlRequest) -> AdapterFuture<'_, ControlState> {
        Box::pin(async move {
            let b = self.get(&session).await?;
            let mut expires = None;
            match r.action {
                ControlRequestAction::Takeover => b.takeover().await.map_err(io)?,
                ControlRequestAction::Release | ControlRequestAction::Revoke => b.release().await,
                ControlRequestAction::Grant => {
                    let seconds = r.lease_seconds.unwrap_or(60.).clamp(1., 3600.);
                    b.grant(
                        r.agent_id.ok_or_else(|| io("agentId required"))?,
                        Duration::from_secs_f64(seconds),
                    )
                    .await
                    .map_err(io)?;
                    expires = Some(
                        (time::OffsetDateTime::now_utc()
                            + time::Duration::milliseconds((seconds * 1000.) as i64))
                        .format(&time::format_description::well_known::Rfc3339)
                        .map_err(io)?,
                    );
                }
            }
            let mut state: ControlState = convert(b.control.snapshot().await)?;
            state.lease_expires_at = expires;
            Ok(state)
        })
    }
}

#[derive(rust_embed::RustEmbed)]
#[folder = "$OUT_DIR/extension"]
struct Extension;
impl Daemon {
    pub async fn real(paths: Paths, port: u16) -> Result<Self> {
        let setup_paths = paths.clone();
        let settings = tokio::task::spawn_blocking(move || -> Result<_> {
            let store = Store::open(setup_paths.clone())?;
            for file in Extension::iter() {
                if let Some(bytes) = Extension::get(&file) {
                    zuruf_store::atomic_write(
                        &setup_paths
                            .data
                            .join("extension-bundle")
                            .join(file.as_ref()),
                        &bytes.data,
                    )?;
                }
            }
            Ok(store.settings()?)
        })
        .await
        .map_err(|e| Error::Join(e.to_string()))??;
        let t3 = T3 {
            inner: AsyncMutex::new(
                zuruf_t3::T3Adapter::new(
                    settings.t3.cli_path.into(),
                    paths.state.join("t3-credentials.json"),
                )
                .map_err(io)?,
            ),
            origin: std::env::var("T3_URL").ok().or(tokio::fs::read_to_string(
                paths.state.join("t3-origin"),
            )
            .await
            .ok()),
        };
        let speech = Arc::new(Speech {
            app: std::sync::OnceLock::new(),
            inner: AsyncMutex::new(SpeechInner {
                backend: None,
                boundary: None,
                started: now(),
                levels: None,
            }),
            directory: paths.state.join("audio"),
            settings: std::sync::RwLock::new(settings.speech),
        });
        let tabs = tokio::fs::read(paths.state.join("tab-ids.json"))
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let browsers = Arc::new(Browsers {
            sessions: AsyncMutex::default(),
            hubs: AsyncMutex::default(),
            bindings: AsyncMutex::new(settings.bindings),
            review: AsyncMutex::new(false),
            paths: paths.clone(),
            port,
            app: std::sync::OnceLock::new(),
            tabs: AsyncMutex::new(tabs),
        });
        let hotkeys = Hotkeys(AsyncMutex::new(
            zuruf_hypr::Hyprland::detect(std::env::current_exe().map_err(StoreError::from)?)
                .await
                .map_err(io)?,
        ));
        let app = Self::open(
            paths,
            Arc::new(t3),
            speech.clone(),
            Arc::new(BrowserAdapter(browsers.clone())),
            Arc::new(hotkeys),
        )
        .await?;
        let _ = speech.app.set(Arc::downgrade(&app.0));
        let _ = browsers.app.set(Arc::downgrade(&app.0));
        let _ = app.0.real.set(Runtime {
            browsers,
            speech,
            access: AsyncMutex::default(),
        });
        let weak = Arc::downgrade(&app.0);
        let reload = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            while let Ok(path) = zuruf_hypr::event_socket() {
                if let Ok(stream) = tokio::net::UnixStream::connect(path).await {
                    let mut lines = BufReader::new(stream).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.starts_with("configreloaded>>") {
                            let Some(inner) = weak.upgrade() else {
                                return;
                            };
                            let daemon = Daemon(inner);
                            if let Ok(snapshot) = daemon.snapshot().await
                                && let Err(error) =
                                    daemon.0.hotkeys.apply(snapshot.settings.bindings).await
                            {
                                let _ = daemon.0.events.send(json!({"type":"notice","payload":{"level":"warning","code":"bindings.reload_conflict","detail":error.to_string()}}));
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        app.0
            .handles
            .lock()
            .map_err(|_| Error::Join("worker lock".into()))?
            .push(reload.abort_handle());
        Ok(app)
    }
}
pub struct Runtime {
    pub browsers: Arc<Browsers>,
    pub speech: Arc<Speech>,
    pub access: AsyncMutex<BTreeMap<String, zuruf_browser::proxy::AgentAccess>>,
}

pub const EXTENSION_ID: &str = "feobfmepfalncljcihpilkecpgkcpmej";
pub fn extension_origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}")
}
pub async fn authorized(app: &Daemon, token: &str) -> bool {
    if token == app.0.token {
        return true;
    }
    if let Some(runtime) = app.0.real.get() {
        return runtime
            .browsers
            .hubs
            .lock()
            .await
            .values()
            .any(|h| h.bootstrap.token == token);
    }
    false
}
pub async fn cors(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let origin = request.headers().get("origin").cloned();
    let extension = origin
        .as_ref()
        .is_some_and(|o| o == extension_origin().as_str());
    if request.method() == Method::OPTIONS && extension {
        return (
            StatusCode::NO_CONTENT,
            [
                ("access-control-allow-origin", extension_origin()),
                (
                    "access-control-allow-methods",
                    "GET, POST, PUT, PATCH, DELETE, OPTIONS".into(),
                ),
                (
                    "access-control-allow-headers",
                    "authorization, content-type".into(),
                ),
                ("vary", "Origin".into()),
            ],
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    if extension {
        response
            .headers_mut()
            .insert("access-control-allow-origin", origin.unwrap());
        response
            .headers_mut()
            .insert("vary", "Origin".parse().unwrap());
    }
    response
}

// Raw upgrades let the browser crate retain ownership of its WS protocol.
pub async fn extension_ws(
    State(app): State<Daemon>,
    mut request: axum::extract::Request,
) -> Result<Response> {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if origin != extension_origin() || app.0.real.get().is_none() {
        return Err(Error::Api(
            StatusCode::FORBIDDEN,
            "extension origin rejected".into(),
        ));
    }
    let key = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| invalid("websocket key required"))?;
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        use futures_util::{SinkExt, StreamExt};
        let Ok(upgraded) = upgrade.await else {
            return;
        };
        let mut ws = tokio_tungstenite::WebSocketStream::from_raw_socket(
            hyper_util::rt::TokioIo::new(upgraded),
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;
        let Ok(Some(Ok(first))) = tokio::time::timeout(Duration::from_secs(3), ws.next()).await
        else {
            return;
        };
        let Ok(auth) = serde_json::from_str::<Value>(first.to_text().unwrap_or("")) else {
            return;
        };
        let Some(runtime) = app.0.real.get() else {
            return;
        };
        let hub = runtime
            .browsers
            .hubs
            .lock()
            .await
            .values()
            .find(|h| auth["type"] == "auth" && auth["token"] == h.bootstrap.token)
            .cloned();
        let Some(hub) = hub else {
            return;
        };
        // Hub consumes its own auth frame: bridge only this already validated socket.
        let (left, right) = tokio::io::duplex(256 * 1024);
        let server = tokio_tungstenite::WebSocketStream::from_raw_socket(
            left,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;
        let mut client = tokio_tungstenite::WebSocketStream::from_raw_socket(
            right,
            tokio_tungstenite::tungstenite::protocol::Role::Client,
            None,
        )
        .await;
        let serving = async {
            let _ = hub.serve(server, &origin, EXTENSION_ID).await;
        };
        let forwarding = async {
            if client.send(first).await.is_err() {
                return;
            }
            loop {
                tokio::select! { frame = ws.next() => match frame { Some(Ok(f)) => if client.send(f).await.is_err() { break; }, _ => break }, frame = client.next() => match frame { Some(Ok(f)) => if ws.send(f).await.is_err() { break; }, _ => break } }
            }
        };
        tokio::select! { _ = serving => {}, _ = forwarding => {} }
    });
    Ok((
        StatusCode::SWITCHING_PROTOCOLS,
        [
            ("upgrade", "websocket"),
            ("connection", "Upgrade"),
            ("sec-websocket-accept", &accept),
        ],
    )
        .into_response())
}

pub async fn agent_access(
    State(app): State<Daemon>,
    Path(session): Path<String>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<Response> {
    if !origin_ok(&headers)
        || headers.get("authorization").and_then(|h| h.to_str().ok())
            != Some(&format!("Bearer {}", app.0.token))
    {
        return Err(Error::Api(
            StatusCode::UNAUTHORIZED,
            "daemon bearer required".into(),
        ));
    }
    let runtime = app.0.real.get().ok_or_else(missing)?;
    if !app
        .snapshot()
        .await?
        .sessions
        .iter()
        .any(|s| s.id == session && s.agent_access)
    {
        return Err(Error::Api(
            StatusCode::FORBIDDEN,
            "agent access disabled".into(),
        ));
    }
    runtime.browsers.get(&session).await?;
    let mut access = runtime.access.lock().await;
    if let Some(old) = access.remove(&session) {
        old.revoke();
    }
    if body["revoke"] == true {
        return Ok(axum::Json(json!({"revoked":true})).into_response());
    }
    let agent = body["agentId"]
        .as_str()
        .ok_or_else(|| invalid("agentId required"))?;
    let token = format!("{}{}", id(), id());
    access.insert(
        session,
        zuruf_browser::proxy::AgentAccess::new(agent.into(), token.clone()),
    );
    Ok(axum::Json(json!({"token":token})).into_response())
}
pub async fn cdp_ws(
    State(app): State<Daemon>,
    Path(session): Path<String>,
    mut request: axum::extract::Request,
) -> Result<Response> {
    let runtime = app.0.real.get().ok_or_else(missing)?;
    let b = runtime.browsers.get(&session).await?;
    let access = runtime
        .access
        .lock()
        .await
        .get(&session)
        .cloned()
        .ok_or_else(|| Error::Api(StatusCode::UNAUTHORIZED, "agent access missing".into()))?;
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    zuruf_browser::proxy::authenticate(Some(&authorization), &access.token)
        .map_err(|_| Error::Api(StatusCode::UNAUTHORIZED, "agent bearer rejected".into()))?;
    access
        .check()
        .map_err(|_| Error::Api(StatusCode::UNAUTHORIZED, "agent access revoked".into()))?;
    let key = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| invalid("websocket key required"))?;
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            let ws = tokio_tungstenite::WebSocketStream::from_raw_socket(
                hyper_util::rt::TokioIo::new(upgraded),
                tokio_tungstenite::tungstenite::protocol::Role::Server,
                None,
            )
            .await;
            let _ = b.serve_agent(ws, access, Some(&authorization)).await;
        }
    });
    Ok((
        StatusCode::SWITCHING_PROTOCOLS,
        [
            ("upgrade", "websocket"),
            ("connection", "Upgrade"),
            ("sec-websocket-accept", &accept),
        ],
    )
        .into_response())
}
