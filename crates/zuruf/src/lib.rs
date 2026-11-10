pub mod fakes;
mod mcp_mount;
pub mod real;
mod worker;
use axum::{
    Router,
    body::Bytes,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocketUpgrade},
    },
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;
use zuruf_core::*;
use zuruf_store::{Paths, Store, StoreError, id, now};
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("{1}")]
    Api(StatusCode, String),
    #[error("background store failure: {0}")]
    Join(String),
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (code, status) = match &self {
            Self::Api(s, _) => (
                if *s == StatusCode::UNAUTHORIZED {
                    "auth.required"
                } else if *s == StatusCode::NOT_FOUND {
                    "not_found"
                } else {
                    "invalid_request"
                },
                *s,
            ),
            Self::Store(StoreError::Invalid(_)) => ("invalid_state", StatusCode::CONFLICT),
            Self::Json(_) => ("invalid_request", StatusCode::UNPROCESSABLE_ENTITY),
            Self::Adapter(_) => ("adapter.failed", StatusCode::BAD_GATEWAY),
            _ => ("internal_error", StatusCode::INTERNAL_SERVER_ERROR),
        };
        tracing::warn!(code,error=%self,"request failed");
        (
            status,
            axum::Json(json!({"error":{"code":code,"message":self.to_string()}})),
        )
            .into_response()
    }
}
fn invalid(s: &str) -> Error {
    Error::Api(StatusCode::BAD_REQUEST, s.into())
}
fn missing() -> Error {
    Error::Api(StatusCode::NOT_FOUND, "entity not found".into())
}
fn val<T: serde::Serialize>(t: T) -> Result<Value> {
    Ok(serde_json::to_value(t)?)
}
struct Persist {
    store: Store,
    snapshot: Snapshot,
}
struct Inner {
    stop: tokio::sync::Notify,
    real: std::sync::OnceLock<real::Runtime>,
    persist: Mutex<Persist>,
    events: broadcast::Sender<Value>,
    token: String,
    codes: Mutex<BTreeMap<String, Instant>>,
    agent_auth: zuruf_mcp::AgentAuth,
    control_gate: tokio::sync::Mutex<()>,
    pub(crate) t3: Arc<dyn T3Adapter>,
    speech: Arc<dyn SpeechAdapter>,
    speech_gate: tokio::sync::Mutex<()>,
    browser: Arc<dyn BrowserController>,
    hotkeys: Arc<dyn DesktopHotkeys>,
    workers: Mutex<HashSet<Id>>,
    handles: Mutex<Vec<tokio::task::AbortHandle>>,
}
#[derive(Clone)]
pub struct Daemon(Arc<Inner>);
impl Daemon {
    pub async fn open(
        paths: Paths,
        t3: Arc<dyn T3Adapter>,
        speech: Arc<dyn SpeechAdapter>,
        browser: Arc<dyn BrowserController>,
        hotkeys: Arc<dyn DesktopHotkeys>,
    ) -> Result<Self> {
        let (store, snapshot, token) = tokio::task::spawn_blocking(move || -> Result<_> {
            let store = Store::open(paths)?;
            let token_path = store.paths.state.join("token");
            let token = if token_path.exists() {
                std::fs::read_to_string(&token_path).map_err(StoreError::from)?
            } else {
                use std::io::Read;
                let mut bytes = [0u8; 32];
                std::fs::File::open("/dev/urandom")
                    .map_err(StoreError::from)?
                    .read_exact(&mut bytes)
                    .map_err(StoreError::from)?;
                let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                zuruf_store::atomic_write(&token_path, token.as_bytes())?;
                token
            };
            let settings = store.settings()?;
            let mut snapshot = store
                .load()?
                .unwrap_or_else(|| empty_snapshot(settings.clone()));
            snapshot.settings = settings;
            Ok((store, snapshot, token))
        })
        .await
        .map_err(|e| Error::Join(e.to_string()))??;
        let (events, _) = broadcast::channel(1024);
        Ok(Self(Arc::new(Inner {
            stop: tokio::sync::Notify::new(),
            real: std::sync::OnceLock::new(),
            persist: Mutex::new(Persist { store, snapshot }),
            events,
            token,
            codes: Mutex::new(BTreeMap::new()),
            agent_auth: zuruf_mcp::AgentAuth::default(),
            control_gate: tokio::sync::Mutex::new(()),
            t3,
            speech,
            speech_gate: tokio::sync::Mutex::new(()),
            browser,
            hotkeys,
            workers: Mutex::new(HashSet::new()),
            handles: Mutex::new(vec![]),
        })))
    }
    pub fn fake(paths: Paths) -> impl std::future::Future<Output = Result<Self>> {
        Self::open(
            paths,
            Arc::new(fakes::FakeT3::default()),
            Arc::new(fakes::FakeSpeech::default()),
            Arc::new(fakes::FakeBrowser::default()),
            Arc::new(fakes::NoopHotkeys),
        )
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/ext", get(real::extension_ws))
            .route("/cdp/{session}", get(real::cdp_ws))
            .route(
                "/api/v1/sessions/{session}/agent-access",
                axum::routing::post(real::agent_access),
            )
            .route("/api/v1/events", get(events))
            .route("/events", get(events))
            .route("/api/v1/{*path}", any(api))
            .fallback(get(ui))
            .layer(axum::middleware::from_fn(real::cors))
            .with_state(self.clone())
            .merge(mcp_mount::router(self))
    }
    pub async fn stopped(&self) {
        self.0.stop.notified().await;
    }
    pub fn shutdown(&self) {
        self.0.stop.notify_waiters();
        if let Ok(handles) = self.0.handles.lock() {
            for h in handles.iter() {
                h.abort();
            }
        }
    }
    pub async fn close(&self) -> Result<()> {
        self.shutdown();
        let mut failure = self.0.speech.cancel().await.err().map(Error::from);
        if let Err(e) = self.0.hotkeys.clear().await {
            failure.get_or_insert(e.into());
        }
        for session in self.snapshot().await?.sessions {
            if let Err(e) = self.0.browser.stop_session(session.id).await {
                failure.get_or_insert(e.into());
            }
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    pub async fn snapshot(&self) -> Result<Snapshot> {
        self.access(|p| Ok(p.snapshot.clone())).await
    }
    pub async fn mint_code(&self) -> Result<String> {
        let code = format!("{}{}", id(), id());
        self.0
            .codes
            .lock()
            .map_err(|_| Error::Join("code lock".into()))?
            .insert(code.clone(), Instant::now() + Duration::from_secs(60));
        Ok(code)
    }
    async fn access<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Persist) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut p = inner
                .persist
                .lock()
                .map_err(|_| Error::Join("store lock".into()))?;
            f(&mut p)
        })
        .await
        .map_err(|e| Error::Join(e.to_string()))?
    }
    async fn mutate<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Snapshot, &mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = self.0.clone();
        self.access(move |p| {
            let before = p.snapshot.clone();
            let mut candidate = before.clone();
            let result = f(&mut candidate, &mut p.store)?;
            candidate.status.speech.pending_segments = candidate
                .segments
                .iter()
                .filter(|s| {
                    matches!(
                        s.state,
                        TranscriptSegmentState::Recording | TranscriptSegmentState::Transcribing
                    )
                })
                .count() as f64;
            let writers = candidate
                .tasks
                .iter()
                .filter(|t| !t.status.terminal())
                .filter_map(|t| t.workspace_path.clone().map(|path| (t.id.clone(), path)))
                .collect::<Vec<_>>();
            for t in &mut candidate.tasks {
                t.workspace_conflict = !t.status.terminal()
                    && t.workspace_path.as_ref().is_some_and(|path| {
                        writers
                            .iter()
                            .any(|(id, other)| id != &t.id && other == path)
                    });
            }
            let mut events = diff(&before, &candidate)?;
            let mut task_events = Vec::new();
            for task in &candidate.tasks {
                let previous = before.tasks.iter().find(|t| t.id == task.id);
                if previous
                    .is_none_or(|t| t.status != task.status || t.last_message != task.last_message)
                {
                    let event = TaskEvent {
                        id: id(),
                        task_id: task.id.clone(),
                        at: now(),
                        kind: if task.error.is_some() {
                            TaskEventKind::Error
                        } else if previous.is_some_and(|t| t.status == task.status) {
                            TaskEventKind::Message
                        } else {
                            TaskEventKind::Status
                        },
                        status: Some(task.status),
                        text: task.error.clone().or(task.last_message.clone()),
                    };
                    events.push(json!({"type":"task.event","payload":event}));
                    task_events.push(event);
                }
            }
            p.store.save_with_events(&candidate, &task_events)?;
            p.snapshot = candidate;
            for e in events {
                let _ = inner.events.send(e);
            }
            Ok(result)
        })
        .await
    }
    pub async fn recover(&self) -> Result<()> {
        self.mutate(|s, _| {
            for segment in &mut s.segments {
                if matches!(
                    segment.state,
                    TranscriptSegmentState::Recording | TranscriptSegmentState::Transcribing
                ) {
                    segment.state = TranscriptSegmentState::Failed;
                }
            }
            Ok(())
        })
        .await?;
        self.mutate(|s, _| {
            let incomplete: HashSet<_> = s.annotations.iter().filter(|a| a.r#type != AnnotationType::Text && a.assets.is_empty()).map(|a| a.batch_id.clone()).collect();
            for task in &mut s.tasks {
                if incomplete.contains(&task.batch_id) && task.thread_id.is_none() && !task.status.terminal() {
                    task.status = TaskStatus::Failed;
                    task.error = Some("Aufnahme durch Neustart unterbrochen. Annotation prüfen und neu aufnehmen.".into());
                }
            }
            Ok(())
        }).await?;
        let connection = self.0.t3.discover_capabilities().await;
        self.mutate(move |s, _| {
            match connection {
                Ok(c) => s.status.t3 = c,
                Err(e) => {
                    s.status.t3.state = T3ConnectionState::Error;
                    s.status.t3.error = Some(e.to_string());
                }
            }
            Ok(())
        })
        .await?;
        for s in self.snapshot().await?.sessions {
            if s.state == BrowserSessionState::Running {
                let id = s.id.clone();
                match self.0.browser.start_session(s).await {
                    Ok(updated) => {
                        self.mutate(move |s, _| {
                            if let Some(session) =
                                s.sessions.iter_mut().find(|s| s.id == updated.id)
                            {
                                *session = updated;
                            }
                            Ok(())
                        })
                        .await?;
                    }
                    Err(error) => {
                        tracing::warn!(error=%error, "session restart failed");
                        self.mutate(move |s, _| {
                            if let Some(session) = s.sessions.iter_mut().find(|s| s.id == id) {
                                session.state = BrowserSessionState::Crashed;
                            }
                            Ok(())
                        })
                        .await?;
                    }
                }
            }
        }
        let snapshot = self.snapshot().await?;
        self.0
            .hotkeys
            .apply(snapshot.settings.bindings.clone())
            .await?;
        self.0
            .browser
            .push_bindings(snapshot.settings.bindings)
            .await?;
        for t in snapshot.tasks {
            if !t.status.terminal() && t.status != TaskStatus::UnknownRemoteState {
                self.start_worker(t.id);
            }
        }
        Ok(())
    }
    pub async fn seed_demo(&self) -> Result<()> {
        if !self.snapshot().await?.projects.is_empty() {
            return Ok(());
        }
        self.mutate(|s, _| {
            let p = Project {
                id: id(),
                name: "Zuruf Demo".into(),
                repository_path: "/tmp/zuruf-demo".into(),
                base_url: "http://localhost:3000".into(),
                t3_project_id: Some("fake-project".into()),
                default_profile_id: None,
                created_at: now(),
                updated_at: now(),
            };
            let profile = AgentProfile {
                id: id(),
                name: "Demo Agent".into(),
                provider_instance_id: "fake-codex".into(),
                model: "demo-model".into(),
                options: BTreeMap::new(),
                runtime_mode: None,
                interaction_mode: None,
                workspace_strategy: WorkspaceStrategy::NewWorktree,
                base_ref: Some("HEAD".into()),
                worktree_path: None,
                slot: Some(1.),
            };
            let mut p = p;
            p.default_profile_id = Some(profile.id.clone());
            s.profiles.push(profile);
            s.projects.push(p.clone());
            s.status.active_project_id = Some(p.id.clone());
            let review = Review {
                id: id(),
                project_id: p.id.clone(),
                name: "Demo Review".into(),
                created_at: now(),
            };
            create_draft(s, &review);
            s.status.active_review_id = Some(review.id.clone());
            s.reviews.push(review);
            let session = BrowserSession {
                id: id(),
                project_id: p.id,
                name: "Demo Browser".into(),
                mode: SessionMode::Shared,
                state: BrowserSessionState::Running,
                agent_access: true,
                control: ControlState {
                    owner: ControlStateOwner::None,
                    agent_id: None,
                    lease_expires_at: None,
                    settling: false,
                },
                tabs: vec![],
                created_at: now(),
            };
            s.status.active_session_id = Some(session.id.clone());
            s.sessions.push(session);
            let b = s.batches[0].id.clone();
            for note in [
                "Button deutlicher beschriften",
                "Abstände auf Mobilgeräten prüfen",
                "Bestätigung nach Speichern anzeigen",
            ] {
                let a = new_annotation(s, &b, AnnotationType::Text, note.into())?;
                s.annotations.push(a);
            }
            Ok(())
        })
        .await?;
        let original = self.snapshot().await?;
        for status in [
            TaskStatus::Completed,
            TaskStatus::Cancelled,
            TaskStatus::Failed,
        ] {
            let profile = original.profiles[0].clone();
            let p = original.projects[0].id.clone();
            self.mutate(move |s, _| {
                let mut b = s.batches[0].clone();
                b.id = id();
                b.state = BatchState::Sealed;
                b.sealed_at = Some(now());
                b.seq = s.batches.len() as f64 + 1.;
                let task = Task {
                    id: id(),
                    batch_id: b.id.clone(),
                    project_id: p,
                    title: format!("Demo {status:?}"),
                    status,
                    remote_status: Some(format!("{status:?}")),
                    profile,
                    provider_display_name: Some("Demo".into()),
                    thread_id: Some(id()),
                    thread_url: None,
                    workspace_path: None,
                    branch: None,
                    last_message: Some("Demo-Ergebnis".into()),
                    error: None,
                    pending_request: None,
                    workspace_conflict: false,
                    marked_done: false,
                    annotation_count: 0.,
                    created_at: now(),
                    dispatched_at: Some(now()),
                    finished_at: None,
                };
                b.task_id = Some(task.id.clone());
                s.batches.push(b);
                s.tasks.push(task);
                Ok(())
            })
            .await?;
        }
        Ok(())
    }
    async fn add_segments(&self, handles: Vec<SegmentHandle>) -> Result<()> {
        for handle in handles {
            let segment = handle.segment;
            let sid = segment.id.clone();
            self.mutate(move |s, _| {
                if let Some(a) = s
                    .annotations
                    .iter_mut()
                    .find(|a| Some(&a.id) == segment.annotation_id.as_ref())
                {
                    a.segment_ids.push(segment.id.clone());
                    a.transcript_state = AnnotationTranscriptState::Transcribing;
                }
                s.segments.push(segment);
                Ok(())
            })
            .await?;
            let app = self.clone();
            let job = tokio::spawn(async move {
                let text = handle.transcript.await;
                let _ = app
                    .mutate(move |s, store| {
                        let seg = s
                            .segments
                            .iter_mut()
                            .find(|s| s.id == sid)
                            .ok_or_else(missing)?;
                        match text {
                            Ok(text) => {
                                let text = if text.chars().any(char::is_alphanumeric) {
                                    text
                                } else {
                                    String::new()
                                };
                                seg.state = if text.is_empty() {
                                    TranscriptSegmentState::Empty
                                } else {
                                    TranscriptSegmentState::Ready
                                };
                                seg.original_text = text.clone();
                                if let Some(batch) = s.batches.iter().find(|b| b.id == seg.batch_id)
                                {
                                    store.write_asset(
                                        batch,
                                        &format!("transcripts/segment-{}.txt", seg.id),
                                        text.as_bytes(),
                                    )?;
                                }
                                if let Some(a) = s
                                    .annotations
                                    .iter_mut()
                                    .find(|a| Some(&a.id) == seg.annotation_id.as_ref())
                                {
                                    if !a.transcript.is_empty() {
                                        a.transcript.push('\n');
                                    }
                                    a.transcript.push_str(&text);
                                    a.transcript_state = AnnotationTranscriptState::Ready;
                                } else if let Some(b) =
                                    s.batches.iter_mut().find(|b| b.id == seg.batch_id)
                                {
                                    b.note.push_str(&text);
                                }
                            }
                            Err(_) => seg.state = TranscriptSegmentState::Failed,
                        }
                        Ok(())
                    })
                    .await;
            });
            if let Ok(mut h) = self.0.handles.lock() {
                h.push(job.abort_handle());
            }
        }
        Ok(())
    }
    async fn dispatch(&self, batch: Id, request: DispatchRequest) -> Result<Value> {
        let guard_id = id();
        let boundary_id = guard_id.clone();
        let result = self
            .mutate(move |s, _| {
                if s.settings.speech.enabled {
                    s.segments.push(TranscriptSegment {
                        id: boundary_id,
                        batch_id: batch.clone(),
                        annotation_id: None,
                        started_at: now(),
                        ended_at: Some(now()),
                        state: TranscriptSegmentState::Recording,
                        original_text: String::new(),
                    });
                }
                Ok(Store::prepare_seal(
                    s,
                    &batch,
                    request.profile_id.as_deref(),
                    request.follow_up_task_id.as_deref(),
                )?)
            })
            .await?;
        let speech_on = self
            .snapshot()
            .await?
            .segments
            .iter()
            .any(|seg| seg.id == guard_id);
        if speech_on {
            let app = self.clone();
            let new_batch = result.active_batch.id.clone();
            let job = tokio::spawn(async move {
                let _gate = app.0.speech_gate.lock().await;
                let outcome = match app.0.speech.stop().await {
                    Ok(handles) => app.add_segments(handles).await,
                    Err(e) => Err(e.into()),
                };
                let successful = outcome.is_ok();
                let error = outcome.err().map(|e| e.to_string());
                let _ = app
                    .mutate(move |s, _| {
                        let guard = s
                            .segments
                            .iter_mut()
                            .find(|seg| seg.id == guard_id)
                            .ok_or_else(missing)?;
                        guard.state = if successful {
                            TranscriptSegmentState::Empty
                        } else {
                            TranscriptSegmentState::Failed
                        };
                        if let Some(error) = error {
                            s.status.speech.state = SpeechStateState::Error;
                            s.status.speech.error = Some(error);
                        }
                        Ok(())
                    })
                    .await;
                if let Err(e) = app
                    .0
                    .speech
                    .start_epoch(SpeechBoundary {
                        batch_id: new_batch,
                        annotation_id: None,
                        at: now(),
                    })
                    .await
                {
                    let _ = app
                        .mutate(move |s, _| {
                            s.status.speech.state = SpeechStateState::Error;
                            s.status.speech.error = Some(e.to_string());
                            Ok(())
                        })
                        .await;
                }
            });
            if let Ok(mut handles) = self.0.handles.lock() {
                handles.retain(|h| !h.is_finished());
                handles.push(job.abort_handle());
            }
        }
        self.start_worker(result.task.id.clone());
        val(result)
    }
    async fn action(&self, action: &str) -> Result<Value> {
        match action {
            "review.toggle" => {
                let on = !self.snapshot().await?.status.review_mode;
                self.set_review(on).await?;
            }
            "batch.dispatch" => {
                let s = self.snapshot().await?;
                let b = s.status.active_batch_id.ok_or_else(missing)?;
                self.dispatch(
                    b,
                    DispatchRequest {
                        profile_id: None,
                        follow_up_task_id: None,
                    },
                )
                .await?;
            }
            "speech.toggle" => {
                let s = self.snapshot().await?;
                let enabled = !s.settings.speech.enabled;
                let _gate = self.0.speech_gate.lock().await;
                if enabled {
                    self.0
                        .speech
                        .start_epoch(SpeechBoundary {
                            batch_id: s.status.active_batch_id.ok_or_else(missing)?,
                            annotation_id: None,
                            at: now(),
                        })
                        .await?;
                } else {
                    self.add_segments(self.0.speech.stop().await?).await?;
                }
                self.mutate(move |s, _| {
                    s.settings.speech.enabled = enabled;
                    s.status.speech.state = if enabled {
                        SpeechStateState::Recording
                    } else {
                        SpeechStateState::Idle
                    };
                    Ok(())
                })
                .await?;
            }
            "capture.viewport" | "capture.region" | "capture.dom" => {
                let s = self.snapshot().await?;
                let b = s.status.active_batch_id.clone().ok_or_else(missing)?;
                let kind = match action {
                    "capture.dom" => AnnotationType::Dom,
                    "capture.region" => AnnotationType::Region,
                    _ => AnnotationType::Viewport,
                };
                let batch_id = b.clone();
                let placeholder = self
                    .mutate(move |s, _| {
                        let existing =
                            if s.settings.capture.mode == SettingsCaptureMode::ExtendOpenEntry {
                                s.annotations
                                    .iter()
                                    .rev()
                                    .find(|a| {
                                        a.batch_id == batch_id && a.status == AnnotationStatus::Open
                                    })
                                    .cloned()
                            } else {
                                None
                            };
                        if let Some(a) = existing {
                            Ok(a)
                        } else {
                            let a = new_annotation(s, &batch_id, kind, String::new())?;
                            s.annotations.push(a.clone());
                            Ok(a)
                        }
                    })
                    .await?;
                if s.settings.speech.enabled {
                    let _gate = self.0.speech_gate.lock().await;
                    let boundary = SpeechBoundary {
                        batch_id: placeholder.batch_id.clone(),
                        annotation_id: Some(placeholder.id.clone()),
                        at: now(),
                    };
                    let mut handles = self.0.speech.mark(boundary).await?;
                    if s.settings.speech.attach == SettingsSpeechAttach::Before {
                        for handle in &mut handles {
                            if handle.segment.batch_id == placeholder.batch_id {
                                handle.segment.annotation_id = Some(placeholder.id.clone());
                            }
                        }
                    }
                    self.add_segments(handles).await?;
                }
                let capture_id = placeholder.id.clone();
                let result = self
                    .0
                    .browser
                    .capture(CaptureRequest {
                        session_id: s.status.active_session_id.ok_or_else(missing)?,
                        tab_id: None,
                        kind,
                        rect: None,
                    })
                    .await?;
                let annotation = self
                    .mutate(move |s, store| {
                        let mut a = s
                            .annotations
                            .iter()
                            .find(|a| a.id == capture_id)
                            .cloned()
                            .ok_or_else(missing)?;
                        a.r#type = kind;
                        a.url = Some(redact_url(&result.tab.url, &s.settings.redaction));
                        a.page_title = Some(result.tab.title);
                        a.session_id = Some(result.tab.session_id);
                        a.tab_id = Some(result.tab.id);
                        a.viewport = result.viewport;
                        a.scroll = result.scroll;
                        a.cursor = result.cursor;
                        a.rect = result.rect;
                        a.dom = result.dom;
                        if let Some(dom) = &mut a.dom {
                            dom.attributes.retain(|key, _| {
                                matches!(
                                    key.as_str(),
                                    "id" | "class"
                                        | "role"
                                        | "data-testid"
                                        | "aria-label"
                                        | "aria-labelledby"
                                        | "type"
                                ) && !sensitive(key, &s.settings.redaction.patterns)
                            });
                        }
                        let batch = s.batches.iter().find(|x| x.id == b).ok_or_else(missing)?;
                        for (n, image) in result.images.into_iter().enumerate() {
                            let asset_id = id();
                            let path = format!("assets/ann-{}-{n}-{asset_id}.png", a.id);
                            store.write_asset(batch, &path, &image.bytes)?;
                            a.assets.push(Asset {
                                id: asset_id.clone(),
                                kind: image.kind,
                                path,
                                width: Some(image.width as f64),
                                height: Some(image.height as f64),
                                url: format!("/api/v1/assets/{asset_id}"),
                            });
                        }
                        if let Some(target) = s.annotations.iter_mut().find(|x| x.id == a.id) {
                            *target = a.clone();
                        }
                        Ok(a)
                    })
                    .await?;
                let _ = annotation;
            }
            "control.takeover" => {
                let s = self
                    .snapshot()
                    .await?
                    .status
                    .active_session_id
                    .ok_or_else(missing)?;
                self.session_control(
                    s,
                    ControlRequest {
                        action: ControlRequestAction::Takeover,
                        agent_id: None,
                        lease_seconds: None,
                    },
                )
                .await?;
            }
            "action.cancel" => {
                self.mutate(|s, _| {
                    s.status.pending_action = None;
                    Ok(())
                })
                .await?;
            }
            "app.open" => {
                tokio::process::Command::new(std::env::current_exe().map_err(StoreError::from)?)
                    .arg("open")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(StoreError::from)?;
            }
            "comment.text" | "panel.toggle" | "batch.send_to_thread" => {
                let a: ActionId = serde_json::from_value(json!(action))?;
                self.mutate(move |s, _| {
                    s.status.pending_action = Some(a);
                    Ok(())
                })
                .await?;
            }
            "profile.slot1" | "profile.slot2" | "profile.slot3" => {
                let slot = action
                    .chars()
                    .last()
                    .and_then(|c| c.to_digit(10))
                    .ok_or_else(|| invalid("slot"))? as f64;
                self.mutate(move |s, _| {
                    let p = s
                        .profiles
                        .iter()
                        .find(|p| p.slot == Some(slot))
                        .ok_or_else(missing)?
                        .id
                        .clone();
                    let b = s
                        .batches
                        .iter_mut()
                        .find(|b| Some(&b.id) == s.status.active_batch_id.as_ref())
                        .ok_or_else(missing)?;
                    b.profile_id = Some(p);
                    Ok(())
                })
                .await?;
            }
            _ => return Err(missing()),
        }
        Ok(json!({"ok":true}))
    }
    async fn set_review(&self, on: bool) -> Result<Value> {
        self.0.browser.set_review_mode(on).await?;
        self.mutate(move |s, _| {
            s.status.review_mode = on;
            val(&s.status)
        })
        .await
    }
    async fn session_control(&self, id: Id, r: ControlRequest) -> Result<Value> {
        let _guard = self.0.control_gate.lock().await;
        if r.action == ControlRequestAction::Revoke
            && let Some(runtime) = self.0.real.get()
            && let Some(access) = runtime.access.lock().await.remove(&id)
        {
            access.revoke();
        }
        let control = self.0.browser.control(id.clone(), r).await?;
        self.mutate(move |s, _| {
            let session = s
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or_else(missing)?;
            session.control = control;
            val(session)
        })
        .await
    }
}
fn empty_snapshot(settings: Settings) -> Snapshot {
    Snapshot {
        status: AppStatus {
            protocol_version: 1.,
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            review_mode: false,
            active_project_id: None,
            active_review_id: None,
            active_batch_id: None,
            active_session_id: None,
            pending_action: None,
            speech: SpeechState {
                state: SpeechStateState::Idle,
                pending_segments: 0.,
                last_gap_ms: None,
                error: None,
            },
            t3: T3Connection {
                state: T3ConnectionState::Disconnected,
                ..fakes::connection()
            },
        },
        projects: vec![],
        profiles: vec![],
        sessions: vec![],
        reviews: vec![],
        batches: vec![],
        annotations: vec![],
        segments: vec![],
        tasks: vec![],
        settings,
    }
}
fn create_draft(s: &mut Snapshot, r: &Review) {
    let b = Batch {
        id: id(),
        review_id: r.id.clone(),
        project_id: r.project_id.clone(),
        seq: s
            .batches
            .iter()
            .filter(|b| b.review_id == r.id)
            .map(|b| b.seq)
            .fold(0., f64::max)
            + 1.,
        state: BatchState::Draft,
        title: "Aufgabe 1".into(),
        note: String::new(),
        profile_id: s
            .projects
            .iter()
            .find(|p| p.id == r.project_id)
            .and_then(|p| p.default_profile_id.clone()),
        task_id: None,
        created_at: now(),
        sealed_at: None,
    };
    s.status.active_batch_id = Some(b.id.clone());
    s.batches.push(b);
}
fn new_annotation(
    s: &Snapshot,
    batch: &str,
    kind: AnnotationType,
    note: String,
) -> Result<Annotation> {
    let b = s
        .batches
        .iter()
        .find(|b| b.id == batch)
        .ok_or_else(missing)?;
    if b.state != BatchState::Draft {
        return Err(invalid("batch is sealed"));
    }
    Ok(Annotation {
        id: id(),
        batch_id: b.id.clone(),
        review_id: b.review_id.clone(),
        project_id: b.project_id.clone(),
        session_id: None,
        tab_id: None,
        r#type: kind,
        order: s.annotations.iter().filter(|a| a.batch_id == batch).count() as f64 + 1.,
        url: None,
        route: None,
        page_title: None,
        viewport: None,
        scroll: None,
        cursor: None,
        rect: None,
        dom: None,
        assets: vec![],
        transcript: String::new(),
        transcript_state: AnnotationTranscriptState::None,
        segment_ids: vec![],
        note,
        priority: AnnotationPriority::Normal,
        tags: vec![],
        status: AnnotationStatus::Open,
        git: None,
        created_at: now(),
        updated_at: now(),
    })
}
fn diff(before: &Snapshot, after: &Snapshot) -> Result<Vec<Value>> {
    let a = val(before)?;
    let b = val(after)?;
    let mut events = vec![];
    for (key, prefix) in [
        ("projects", "project"),
        ("profiles", "profile"),
        ("sessions", "session"),
        ("reviews", "review"),
        ("batches", "batch"),
        ("annotations", "annotation"),
        ("segments", "segment"),
        ("tasks", "task"),
    ] {
        let old = a[key].as_array().ok_or_else(|| invalid("snapshot"))?;
        let new = b[key].as_array().ok_or_else(|| invalid("snapshot"))?;
        for item in new {
            if !old.contains(item) {
                events.push(json!({"type":format!("{prefix}.upserted"),"payload":item}));
            }
        }
        for item in old {
            if !new.iter().any(|n| n["id"] == item["id"]) {
                let mut payload = json!({"id":item["id"]});
                if prefix == "annotation" {
                    payload["batchId"] = item["batchId"].clone();
                }
                events.push(json!({"type":format!("{prefix}.removed"),"payload":payload}));
            }
        }
    }
    for (key, event) in [
        ("status", "status.changed"),
        ("settings", "settings.changed"),
    ] {
        if a[key] != b[key] {
            events.push(json!({"type":event,"payload":b[key]}));
        }
    }
    Ok(events)
}
fn origin_ok(headers: &HeaderMap) -> bool {
    headers.get("origin").is_none_or(|o| {
        o.to_str().is_ok_and(|origin| {
            let host = headers
                .get("host")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");
            origin == format!("http://{host}")
                || origin == real::extension_origin()
                || std::env::var("ZURUF_EXTENSION_ORIGIN").is_ok_and(|allowed| allowed == origin)
        })
    })
}
async fn events(
    State(app): State<Daemon>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    if !origin_ok(&headers) {
        return Err(Error::Api(StatusCode::FORBIDDEN, "origin rejected".into()));
    }
    Ok(ws
        .on_upgrade(move |socket| event_socket(app, socket))
        .into_response())
}
async fn stream_snapshot(app: &Daemon) -> Result<(Snapshot, broadcast::Receiver<Value>)> {
    let source = app.0.clone();
    app.access(move |p| Ok((p.snapshot.clone(), source.events.subscribe())))
        .await
}
async fn event_socket(app: Daemon, mut socket: axum::extract::ws::WebSocket) {
    let auth = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let accepted = match auth {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<Value>(&text) {
            Ok(v) if v["type"] == "auth" => {
                real::authorized(&app, v["token"].as_str().unwrap_or("")).await
            }
            _ => false,
        },
        _ => false,
    };
    if !accepted {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let Ok((snapshot, mut rx)) = stream_snapshot(&app).await else {
        return;
    };
    let mut seq = 1u64;
    let first = json!({"type":"snapshot","payload":snapshot,"seq":seq});
    if socket
        .send(Message::Text(first.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            event=rx.recv()=>{
                let mut event=match event{
                    Ok(event)=>event,
                    Err(broadcast::error::RecvError::Lagged(_))=>{
                        let Ok((snapshot,new_rx))=stream_snapshot(&app).await else{break};
                        rx=new_rx;
                        json!({"type":"snapshot","payload":snapshot})
                    }
                    Err(_)=>break,
                };
                seq+=1;
                event["seq"]=json!(seq);
                if socket.send(Message::Text(event.to_string().into())).await.is_err(){break;}
            }
            _=app.stopped()=>break,
            frame=socket.recv()=>{
                match frame{
                    Some(Ok(Message::Ping(bytes)))=>{
                        if socket.send(Message::Pong(bytes)).await.is_err(){break;}
                    }
                    Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,
                    _=>{}
                }
            }
        }
    }
}
#[derive(rust_embed::RustEmbed)]
#[folder = "$OUT_DIR/ui"]
struct Ui;
async fn ui(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Ui::get(path) {
        let mime = if path.ends_with(".js") {
            "text/javascript"
        } else if path.ends_with(".css") {
            "text/css"
        } else if path.ends_with(".svg") {
            "image/svg+xml"
        } else {
            "text/html; charset=utf-8"
        };
        return ([("content-type", mime)], file.data).into_response();
    }
    axum::response::Html("<!doctype html><html lang=de><meta charset=utf-8><title>Zuruf</title><p>Zuruf läuft. Das UI-Bundle ist noch nicht gebaut.</p></html>").into_response()
}
async fn api(
    State(app): State<Daemon>,
    Path(path): Path<String>,
    Query(query): Query<BTreeMap<String, String>>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    if !origin_ok(&headers) {
        return Err(Error::Api(StatusCode::FORBIDDEN, "origin rejected".into()));
    }
    let v: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body)?
    };
    if path == "auth/exchange" && method == Method::POST {
        let code = v["code"].as_str().ok_or_else(|| invalid("code required"))?;
        let expiry = app
            .0
            .codes
            .lock()
            .map_err(|_| Error::Join("codes".into()))?
            .remove(code);
        if expiry.is_none_or(|e| e < Instant::now()) {
            return Err(Error::Api(StatusCode::UNAUTHORIZED, "invalid code".into()));
        }
        return Ok(axum::Json(json!({"token":app.0.token})).into_response());
    }
    let auth = headers.get("authorization").and_then(|h| h.to_str().ok());
    if !real::authorized(
        &app,
        auth.and_then(|a| a.strip_prefix("Bearer ")).unwrap_or(""),
    )
    .await
    {
        return Err(Error::Api(
            StatusCode::UNAUTHORIZED,
            "bearer required".into(),
        ));
    }
    let parts = path.split('/').collect::<Vec<_>>();
    let entity = parts.first().copied().unwrap_or("");
    let key = parts.get(1).copied().unwrap_or("").to_string();
    let operation = parts.get(2).copied().unwrap_or("");
    let result = match (method.as_str(), entity, key.as_str(), operation) {
        ("POST", "shutdown", _, _) => {
            app.0.stop.notify_one();
            json!({"ok":true})
        }
        ("GET", "state", _, _) => val(app.snapshot().await?)?,
        ("POST", "auth", "code", _) => json!({"code":app.mint_code().await?}),
        ("POST", "actions", _, _) => app.action(&key).await?,
        ("POST", "review-mode", _, _) => {
            app.set_review(v["on"].as_bool().ok_or_else(|| invalid("on required"))?)
                .await?
        }
        ("POST", "active", _, _) => {
            app.mutate(move |s, _| {
                for (field, target) in [
                    ("projectId", &mut s.status.active_project_id),
                    ("reviewId", &mut s.status.active_review_id),
                    ("sessionId", &mut s.status.active_session_id),
                ] {
                    if let Some(value) = v.get(field) {
                        *target = value.as_str().map(str::to_string);
                    }
                }
                if let Some(review) = &s.status.active_review_id {
                    s.status.active_batch_id = s
                        .batches
                        .iter()
                        .find(|b| &b.review_id == review && b.state == BatchState::Draft)
                        .map(|b| b.id.clone());
                }
                val(&s.status)
            })
            .await?
        }
        ("GET", "t3", "catalog", _) => val(app.0.t3.catalog(query.get("search").cloned()).await?)?,
        ("GET", "t3", "projects", _) => val(app.0.t3.list_projects().await?)?,
        ("POST", "t3", "reconnect", _) => {
            let c = app.0.t3.discover_capabilities().await?;
            app.mutate(move |s, _| {
                s.status.t3 = c;
                val(&s.status.t3)
            })
            .await?
        }
        ("GET", "settings", _, _) => val(app.snapshot().await?.settings)?,
        ("POST", "settings", "bindings", "check") => {
            let bindings: Vec<Binding> = serde_json::from_value(v)?;
            let mut issues = validate_bindings(&bindings);
            issues.extend(app.0.hotkeys.check_conflicts(bindings).await?);
            val(issues)?
        }
        ("PUT", "settings", _, _) => {
            let settings: Settings = serde_json::from_value(v)?;
            let mut issues = validate_bindings(&settings.bindings);
            issues.extend(
                app.0
                    .hotkeys
                    .check_conflicts(settings.bindings.clone())
                    .await?,
            );
            if !issues.is_empty() {
                return Ok((StatusCode::UNPROCESSABLE_ENTITY,axum::Json(json!({"error":{"code":"bindings.invalid","message":"Ungültige Tastenbelegung","issues":issues}}))).into_response());
            }
            if let Some(runtime) = app.0.real.get() {
                *runtime
                    .speech
                    .settings
                    .write()
                    .map_err(|_| Error::Join("speech settings lock".into()))? =
                    settings.speech.clone();
            }
            app.0.hotkeys.apply(settings.bindings.clone()).await?;
            app.0
                .browser
                .push_bindings(settings.bindings.clone())
                .await?;
            app.mutate(move |s, store| {
                store.save_settings(&settings)?;
                s.settings = settings;
                val(&s.settings)
            })
            .await?
        }
        ("POST", "reviews", _, _) => {
            app.mutate(move |s, _| {
                let project = v["projectId"]
                    .as_str()
                    .ok_or_else(|| invalid("projectId required"))?;
                if !s.projects.iter().any(|p| p.id == project) {
                    return Err(missing());
                }
                let r = Review {
                    id: id(),
                    project_id: project.into(),
                    name: v["name"].as_str().unwrap_or("Review").into(),
                    created_at: now(),
                };
                create_draft(s, &r);
                s.status.active_review_id = Some(r.id.clone());
                s.reviews.push(r.clone());
                val(r)
            })
            .await?
        }
        ("POST", "batches", _, "dispatch") => app.dispatch(key, serde_json::from_value(v)?).await?,
        ("POST", "batches", _, "discard") => {
            app.mutate(move |s, _| {
                let b = s
                    .batches
                    .iter_mut()
                    .find(|b| b.id == key)
                    .ok_or_else(missing)?;
                b.state = b
                    .state
                    .transition(BatchState::Discarded)
                    .map_err(|e| invalid(&e.to_string()))?;
                let r = s
                    .reviews
                    .iter()
                    .find(|r| r.id == b.review_id)
                    .cloned()
                    .ok_or_else(missing)?;
                let result = val(b)?;
                create_draft(s, &r);
                Ok(result)
            })
            .await?
        }
        ("PATCH", "batches", _, _) => {
            app.mutate(move |s, _| {
                let b = s
                    .batches
                    .iter_mut()
                    .find(|b| b.id == key)
                    .ok_or_else(missing)?;
                if b.state != BatchState::Draft {
                    return Err(invalid("sealed batch"));
                }
                patch(b, v, &["title", "note", "profileId"])
            })
            .await?
        }
        ("POST", "annotations", _, _) => {
            app.mutate(move |s, _| {
                if v["type"] != "text" {
                    return Err(invalid("only text annotations may be posted"));
                }
                let a = new_annotation(
                    s,
                    v["batchId"]
                        .as_str()
                        .ok_or_else(|| invalid("batchId required"))?,
                    AnnotationType::Text,
                    v["note"].as_str().unwrap_or("").into(),
                )?;
                s.annotations.push(a.clone());
                val(a)
            })
            .await?
        }
        ("PATCH", "annotations", _, _) => {
            app.mutate(move |s, _| {
                let index = s
                    .annotations
                    .iter()
                    .position(|a| a.id == key)
                    .ok_or_else(missing)?;
                if !s
                    .batches
                    .iter()
                    .any(|b| b.id == s.annotations[index].batch_id && b.state == BatchState::Draft)
                {
                    return Err(invalid("sealed annotation; use follow-up"));
                }
                if let Some(dest) = v["batchId"].as_str() {
                    let b = s
                        .batches
                        .iter()
                        .find(|b| {
                            b.id == dest
                                && b.state == BatchState::Draft
                                && b.project_id == s.annotations[index].project_id
                        })
                        .ok_or_else(|| invalid("invalid target batch"))?;
                    s.annotations[index].review_id = b.review_id.clone();
                }
                s.annotations[index].updated_at = now();
                patch(
                    &mut s.annotations[index],
                    v,
                    &[
                        "transcript",
                        "note",
                        "priority",
                        "tags",
                        "status",
                        "order",
                        "batchId",
                    ],
                )
            })
            .await?
        }
        ("DELETE", "annotations", _, _) => {
            app.mutate(move |s, _| {
                let a = s
                    .annotations
                    .iter()
                    .find(|a| a.id == key)
                    .ok_or_else(missing)?;
                if !s
                    .batches
                    .iter()
                    .any(|b| b.id == a.batch_id && b.state == BatchState::Draft)
                {
                    return Err(invalid("sealed annotation"));
                }
                s.annotations.retain(|a| a.id != key);
                Ok(json!({"ok":true}))
            })
            .await?
        }
        ("GET", "assets", _, _) => {
            let bytes = app
                .access(move |p| {
                    for a in &p.snapshot.annotations {
                        if let Some(asset) = a.assets.iter().find(|a| a.id == key) {
                            let b = p
                                .snapshot
                                .batches
                                .iter()
                                .find(|b| b.id == a.batch_id)
                                .ok_or_else(missing)?;
                            let path =
                                zuruf_store::safe_join(&p.store.paths.batch_dir(b)?, &asset.path)?;
                            return Ok(std::fs::read(path).map_err(StoreError::from)?);
                        }
                    }
                    Err(missing())
                })
                .await?;
            return Ok(([("content-type", "image/png")], bytes).into_response());
        }
        ("GET", "tasks", "", _) => {
            let s = app.snapshot().await?;
            val(s
                .tasks
                .into_iter()
                .filter(|t| query.get("projectId").is_none_or(|p| &t.project_id == p))
                .take(
                    query
                        .get("limit")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(100),
                )
                .collect::<Vec<_>>())?
        }
        ("GET", "tasks", _, "events") => app.access(move |p| val(p.store.events(&key)?)).await?,
        ("PATCH", "tasks", _, _) => {
            app.mutate(move |s, _| {
                let t = s
                    .tasks
                    .iter_mut()
                    .find(|t| t.id == key)
                    .ok_or_else(missing)?;
                patch(t, v, &["markedDone"])
            })
            .await?
        }
        ("POST", "tasks", _, "retry") => {
            let target = key.clone();
            let result = app
                .mutate(move |s, store| {
                    let t = s
                        .tasks
                        .iter_mut()
                        .find(|t| t.id == key)
                        .ok_or_else(missing)?;
                    if t.status == TaskStatus::UnknownRemoteState {
                        return Err(invalid("resolve ambiguous outcome first"));
                    }
                    if !matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled) {
                        return Err(invalid("task cannot be retried"));
                    }
                    store.clear_receipt(&t.id)?;
                    t.status = TaskStatus::Queued;
                    t.error = None;
                    val(t)
                })
                .await?;
            app.start_worker(target);
            result
        }
        ("POST", "tasks", _, "cancel") => {
            let t = app
                .snapshot()
                .await?
                .tasks
                .into_iter()
                .find(|t| t.id == key)
                .ok_or_else(missing)?;
            if let Some(thread) = t.thread_id {
                app.0.t3.interrupt(thread).await?;
            }
            app.mutate(move |s, _| {
                let t = s
                    .tasks
                    .iter_mut()
                    .find(|t| t.id == key)
                    .ok_or_else(missing)?;
                if t.status.terminal() {
                    return Err(invalid("task already terminal"));
                }
                t.status = TaskStatus::Cancelled;
                t.finished_at = Some(now());
                val(t)
            })
            .await?
        }
        ("POST", "tasks", _, "resolve") => {
            let r: ResolveRequest = serde_json::from_value(v)?;
            let target = key.clone();
            let result = app
                .mutate(move |s, store| {
                    let t = s
                        .tasks
                        .iter_mut()
                        .find(|t| t.id == key)
                        .ok_or_else(missing)?;
                    if t.status != TaskStatus::UnknownRemoteState {
                        return Err(invalid("task not ambiguous"));
                    }
                    match r.resolution {
                        ResolveRequestResolution::NotSent => {
                            t.status = TaskStatus::Queued;
                            t.thread_id = None;
                        }
                        ResolveRequestResolution::AdoptThread => {
                            let thread = r.thread_id.ok_or_else(|| invalid("threadId required"))?;
                            t.thread_id = Some(thread.clone());
                            t.status = TaskStatus::Running;
                            store.save_receipt(
                                &t.id,
                                &ThreadReceipt {
                                    thread_id: thread,
                                    thread_url: None,
                                    run_id: String::new(),
                                    workspace_path: None,
                                    branch: None,
                                },
                            )?;
                        }
                    }
                    t.error = None;
                    val(t)
                })
                .await?;
            app.start_worker(target);
            result
        }
        ("POST", "tasks", _, "follow-up") => {
            let request: FollowUpRequest = serde_json::from_value(v)?;
            let task_id = key.clone();
            let inner = app.0.clone();
            let result = app
                .access(move |p| {
                    let before = p.snapshot.clone();
                    let mut next = before.clone();
                    let task = next
                        .tasks
                        .iter()
                        .find(|t| t.id == task_id)
                        .cloned()
                        .ok_or_else(missing)?;
                    if !task.status.terminal() {
                        return Err(invalid("wait for the current run before sending follow-up"));
                    }
                    if task.thread_id.is_none() {
                        return Err(invalid("thread required"));
                    }
                    let request_id = id();
                    let mut prompt = request.text.unwrap_or_default();
                    for annotation_id in request.annotation_ids.unwrap_or_default() {
                        let a = next
                            .annotations
                            .iter()
                            .find(|a| a.id == annotation_id && a.project_id == task.project_id)
                            .cloned()
                            .ok_or_else(missing)?;
                        let batch = next
                            .batches
                            .iter()
                            .find(|b| b.id == a.batch_id)
                            .ok_or_else(missing)?;
                        let project = next
                            .projects
                            .iter()
                            .find(|pr| pr.id == task.project_id)
                            .ok_or_else(missing)?;
                        let manifest = build_manifest(
                            batch,
                            project,
                            &task.profile,
                            &[a],
                            &next.settings.redaction,
                        )
                        .map_err(|e| invalid(&e.to_string()))?;
                        let path = p.store.write_asset(
                            batch,
                            &format!("follow-up-{request_id}-{annotation_id}.json"),
                            &serde_json::to_vec_pretty(&manifest)?,
                        )?;
                        prompt.push('\n');
                        prompt.push_str(
                            &build_prompt(&path.to_string_lossy())
                                .map_err(|e| invalid(&e.to_string()))?,
                        );
                    }
                    if prompt.trim().is_empty() {
                        return Err(invalid("empty follow-up"));
                    }
                    let t = next
                        .tasks
                        .iter_mut()
                        .find(|t| t.id == task_id)
                        .ok_or_else(missing)?;
                    t.status = TaskStatus::Queued;
                    t.finished_at = None;
                    t.error = None;
                    let response = val(&*t)?;
                    let event = TaskEvent {
                        id: request_id.clone(),
                        task_id: task_id.clone(),
                        at: now(),
                        kind: TaskEventKind::FollowUp,
                        status: Some(TaskStatus::Queued),
                        text: Some(prompt.clone()),
                    };
                    let message = zuruf_store::QueuedMessage {
                        task_id,
                        request_id,
                        prompt,
                    };
                    p.store.save_with_message(
                        &next,
                        std::slice::from_ref(&event),
                        Some(&message),
                    )?;
                    p.snapshot = next.clone();
                    for e in diff(&before, &next)? {
                        let _ = inner.events.send(e);
                    }
                    let _ = inner
                        .events
                        .send(json!({"type":"task.event","payload":event}));
                    Ok(response)
                })
                .await?;
            app.start_worker(key);
            result
        }
        ("POST", "sessions", _, "control") => {
            app.session_control(key, serde_json::from_value(v)?).await?
        }
        ("POST", "sessions", _, "start" | "stop" | "restart" | "duplicate" | "tabs") => {
            let s = app
                .snapshot()
                .await?
                .sessions
                .into_iter()
                .find(|s| s.id == key)
                .ok_or_else(missing)?;
            let updated = match operation {
                "start" => app.0.browser.start_session(s).await?,
                "restart" => {
                    app.0.browser.stop_session(key.clone()).await?;
                    app.0.browser.start_session(s).await?
                }
                "stop" => {
                    app.0.browser.stop_session(key.clone()).await?;
                    BrowserSession {
                        state: BrowserSessionState::Stopped,
                        ..s
                    }
                }
                "duplicate" => app.0.browser.duplicate_session(s, id()).await?,
                _ => {
                    let tab = app
                        .0
                        .browser
                        .open_tab(
                            key.clone(),
                            v["url"]
                                .as_str()
                                .ok_or_else(|| invalid("url required"))?
                                .into(),
                        )
                        .await?;
                    let mut s = s;
                    for t in &mut s.tabs {
                        t.active = false;
                    }
                    s.tabs.push(tab);
                    s
                }
            };
            app.mutate(move |s, _| {
                s.sessions.retain(|x| x.id != updated.id);
                s.sessions.push(updated.clone());
                val(updated)
            })
            .await?
        }
        ("GET", "projects" | "profiles", "", _) => {
            let s = val(app.snapshot().await?)?;
            s[entity].clone()
        }
        ("POST", "projects" | "profiles" | "sessions", _, _) => {
            let entity = entity.to_string();
            app.mutate(move |s, _| {
                let mut item = v;
                item["id"] = json!(id());
                if entity == "projects" {
                    item["createdAt"] = json!(now());
                    item["updatedAt"] = json!(now());
                    let p: Project = serde_json::from_value(item)?;
                    s.status.active_project_id = Some(p.id.clone());
                    s.projects.push(p.clone());
                    val(p)
                } else if entity == "profiles" {
                    let p: AgentProfile = serde_json::from_value(item)?;
                    s.profiles.push(p.clone());
                    val(p)
                } else {
                    item["state"] = json!("stopped");
                    item["control"] = json!({"owner":"none","settling":false});
                    item["tabs"] = json!([]);
                    item["createdAt"] = json!(now());
                    let p: BrowserSession = serde_json::from_value(item)?;
                    if !s.projects.iter().any(|x| x.id == p.project_id) {
                        return Err(missing());
                    }
                    s.status.active_session_id = Some(p.id.clone());
                    s.sessions.push(p.clone());
                    val(p)
                }
            })
            .await?
        }
        ("PATCH", "projects" | "profiles" | "sessions", _, _) => {
            let entity = entity.to_string();
            app.mutate(move |s, _| match entity.as_str() {
                "projects" => {
                    let p = s
                        .projects
                        .iter_mut()
                        .find(|p| p.id == key)
                        .ok_or_else(missing)?;
                    p.updated_at = now();
                    patch(
                        p,
                        v,
                        &[
                            "name",
                            "repositoryPath",
                            "baseUrl",
                            "t3ProjectId",
                            "defaultProfileId",
                        ],
                    )
                }
                "profiles" => {
                    let p = s
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == key)
                        .ok_or_else(missing)?;
                    patch(
                        p,
                        v,
                        &[
                            "name",
                            "providerInstanceId",
                            "model",
                            "options",
                            "runtimeMode",
                            "interactionMode",
                            "workspaceStrategy",
                            "baseRef",
                            "worktreePath",
                            "slot",
                        ],
                    )
                }
                _ => {
                    let p = s
                        .sessions
                        .iter_mut()
                        .find(|p| p.id == key)
                        .ok_or_else(missing)?;
                    patch(p, v, &["name", "mode", "agentAccess"])
                }
            })
            .await?
        }
        ("DELETE", "projects" | "profiles" | "sessions", _, _) => {
            if entity == "sessions" {
                app.0.browser.stop_session(key.clone()).await?;
            }
            let entity = entity.to_string();
            app.mutate(move |s, _| {
                let removed = match entity.as_str() {
                    "projects" => {
                        if s.reviews.iter().any(|r| r.project_id == key)
                            || s.sessions.iter().any(|r| r.project_id == key)
                        {
                            return Err(invalid("project still referenced"));
                        }
                        let p = s
                            .projects
                            .iter()
                            .find(|p| p.id == key)
                            .cloned()
                            .ok_or_else(missing)?;
                        s.projects.retain(|p| p.id != key);
                        val(p)?
                    }
                    "profiles" => {
                        let p = s
                            .profiles
                            .iter()
                            .find(|p| p.id == key)
                            .cloned()
                            .ok_or_else(missing)?;
                        s.profiles.retain(|p| p.id != key);
                        val(p)?
                    }
                    _ => {
                        let p = s
                            .sessions
                            .iter()
                            .find(|p| p.id == key)
                            .cloned()
                            .ok_or_else(missing)?;
                        s.sessions.retain(|p| p.id != key);
                        val(p)?
                    }
                };
                Ok(removed)
            })
            .await?
        }
        _ => return Err(missing()),
    };
    Ok(axum::Json(result).into_response())
}
fn patch<T: serde::Serialize + serde::de::DeserializeOwned>(
    target: &mut T,
    patch: Value,
    allowed: &[&str],
) -> Result<Value> {
    let mut item = val(&*target)?;
    let object = patch
        .as_object()
        .ok_or_else(|| invalid("object required"))?;
    for (k, v) in object {
        if !allowed.contains(&k.as_str()) {
            return Err(invalid("field cannot be changed"));
        }
        item[k] = v.clone();
    }
    *target = serde_json::from_value(item)?;
    val(target)
}
