use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use zuruf_core::*;
use zuruf_store::{id, now};
#[derive(Clone, Copy)]
pub enum FakeMode {
    Normal,
    Failure,
    TimeoutBefore,
    TimeoutAfter,
    Approval,
    Question,
}
struct Entry {
    request: CreateThreadRequest,
    receipt: ThreadReceipt,
    started: Instant,
    cancelled: bool,
}
#[derive(Default)]
pub struct FakeT3 {
    mode: AtomicU8,
    threads: Mutex<BTreeMap<String, Entry>>,
}
impl FakeT3 {
    pub fn set_mode(&self, mode: FakeMode) {
        self.mode.store(mode as u8, Ordering::SeqCst);
    }
    pub fn thread_count(&self) -> usize {
        self.threads.lock().map(|t| t.len()).unwrap_or_default()
    }
}
fn lock_error<T>(_: std::sync::PoisonError<T>) -> AdapterError {
    AdapterError::Io("fake lock poisoned".into())
}
pub fn connection() -> T3Connection {
    T3Connection {
        state: T3ConnectionState::Connected,
        version: Some("fake-1".into()),
        transport: Some(T3ConnectionTransport::Mcp),
        capabilities: T3ConnectionCapabilities {
            model_options: true,
            existing_worktree: true,
            idempotent_launch: true,
            interrupt: true,
            attachments: false,
            push: false,
        },
        error: None,
    }
}
impl T3Adapter for FakeT3 {
    fn discover_capabilities(&self) -> AdapterFuture<'_, T3Connection> {
        Box::pin(async { Ok(connection()) })
    }
    fn list_projects(&self) -> AdapterFuture<'_, Vec<T3ProjectRef>> {
        Box::pin(async {
            Ok(vec![T3ProjectRef {
                id: "fake-project".into(),
                title: "Demo".into(),
                workspace_root: "/tmp/zuruf-demo".into(),
            }])
        })
    }
    fn catalog(&self, search: Option<String>) -> AdapterFuture<'_, T3Catalog> {
        Box::pin(async move {
            Ok(T3Catalog {
                fetched_at: now(),
                providers: ["fake-codex", "fake-claude"]
                    .into_iter()
                    .map(|p| T3Provider {
                        instance_id: p.into(),
                        display_name: p.into(),
                        enabled: true,
                        constraints: vec![],
                        models: if search.as_deref().is_none_or(|s| "demo-model".contains(s)) {
                            vec![T3Model {
                                id: "demo-model".into(),
                                label: Some("Demo model".into()),
                                options: vec![],
                            }]
                        } else {
                            vec![]
                        },
                        omitted_models: 0.,
                    })
                    .collect(),
                runtime_modes: vec!["default".into()],
                interaction_modes: vec!["default".into()],
            })
        })
    }
    fn create_thread(&self, input: CreateThreadRequest) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            let mode = self.mode.load(Ordering::SeqCst);
            if mode == FakeMode::Failure as u8 {
                return Err(AdapterError::Unavailable("fake connection failure".into()));
            }
            if mode == FakeMode::TimeoutBefore as u8 {
                return Err(AdapterError::Ambiguous("fake timeout".into()));
            }
            let mut threads = self.threads.lock().map_err(lock_error)?;
            if let Some(e) = threads
                .values()
                .find(|e| e.request.client_request_id == input.client_request_id)
            {
                return Ok(e.receipt.clone());
            }
            let thread = id();
            let receipt = ThreadReceipt {
                thread_id: thread.clone(),
                thread_url: Some(format!("http://127.0.0.1/fake/{thread}")),
                run_id: id(),
                workspace_path: Some(match input.profile.workspace_strategy {
                    WorkspaceStrategy::NewWorktree => {
                        format!("/tmp/zuruf-demo/{}", input.client_request_id)
                    }
                    WorkspaceStrategy::ExistingWorktree => input
                        .profile
                        .worktree_path
                        .clone()
                        .unwrap_or_else(|| input.project.repository_path.clone()),
                    WorkspaceStrategy::SharedRoot => input.project.repository_path.clone(),
                }),
                branch: Some(format!("zuruf/{}", input.client_request_id)),
            };
            threads.insert(
                thread,
                Entry {
                    request: input,
                    receipt: receipt.clone(),
                    started: Instant::now(),
                    cancelled: false,
                },
            );
            if mode == FakeMode::TimeoutAfter as u8 {
                Err(AdapterError::Ambiguous("receipt lost".into()))
            } else {
                Ok(receipt)
            }
        })
    }
    fn send_message(
        &self,
        thread: String,
        _request: String,
        _prompt: String,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        Box::pin(async move {
            let mut threads = self.threads.lock().map_err(lock_error)?;
            let e = threads
                .get_mut(&thread)
                .ok_or_else(|| AdapterError::Rejected("thread missing".into()))?;
            e.started = Instant::now();
            e.receipt.run_id = id();
            Ok(e.receipt.clone())
        })
    }
    fn thread_status(&self, thread: String, run: String) -> AdapterFuture<'_, RemoteStatus> {
        Box::pin(async move {
            let threads = self.threads.lock().map_err(lock_error)?;
            let e = threads.get(&thread).ok_or_else(|| {
                AdapterError::Unavailable("thread absent after fake restart".into())
            })?;
            let mode = self.mode.load(Ordering::SeqCst);
            let status = if e.cancelled {
                "cancelled"
            } else if mode == FakeMode::Approval as u8 || mode == FakeMode::Question as u8 {
                "waiting"
            } else if e.started.elapsed() > Duration::from_millis(800) {
                "completed"
            } else {
                "running"
            };
            Ok(RemoteStatus {
                error: None,
                run_id: run,
                status: status.into(),
                last_assistant_message: if status == "completed" {
                    Some("Änderungen je Annotation geprüft.".into())
                } else {
                    None
                },
                pending_request: if status == "waiting" {
                    Some("Bitte bestätigen.".into())
                } else {
                    None
                },
                approval: mode == FakeMode::Approval as u8,
            })
        })
    }
    fn find_threads_by_marker(
        &self,
        project: Project,
        marker: String,
    ) -> AdapterFuture<'_, Vec<ThreadReceipt>> {
        Box::pin(async move {
            Ok(self
                .threads
                .lock()
                .map_err(lock_error)?
                .values()
                .filter(|e| e.request.project.id == project.id && e.request.title.contains(&marker))
                .map(|e| e.receipt.clone())
                .collect())
        })
    }
    fn interrupt(&self, thread: String) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            if let Some(e) = self.threads.lock().map_err(lock_error)?.get_mut(&thread) {
                e.cancelled = true;
            }
            Ok(())
        })
    }
}
#[derive(Default)]
pub struct FakeSpeech {
    boundary: Mutex<Option<SpeechBoundary>>,
}
impl SpeechAdapter for FakeSpeech {
    fn state(&self) -> AdapterFuture<'_, SpeechState> {
        Box::pin(async {
            Ok(SpeechState {
                state: SpeechStateState::Idle,
                pending_segments: 0.,
                last_gap_ms: None,
                error: None,
            })
        })
    }
    fn start_epoch(&self, b: SpeechBoundary) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            *self.boundary.lock().map_err(lock_error)? = Some(b);
            Ok(())
        })
    }
    fn mark(&self, b: SpeechBoundary) -> AdapterFuture<'_, Vec<SegmentHandle>> {
        Box::pin(async move {
            let old = self.boundary.lock().map_err(lock_error)?.replace(b);
            Ok(old
                .into_iter()
                .map(|b| SegmentHandle {
                    segment: TranscriptSegment {
                        id: id(),
                        batch_id: b.batch_id,
                        annotation_id: b.annotation_id,
                        started_at: b.at,
                        ended_at: Some(now()),
                        state: TranscriptSegmentState::Transcribing,
                        original_text: String::new(),
                    },
                    transcript: Box::pin(async {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        Ok("Bitte diesen Bereich verbessern.".into())
                    }),
                })
                .collect())
        })
    }
    fn stop(&self) -> AdapterFuture<'_, Vec<SegmentHandle>> {
        Box::pin(async move {
            let old = self.boundary.lock().map_err(lock_error)?.take();
            Ok(old
                .into_iter()
                .map(|b| SegmentHandle {
                    segment: TranscriptSegment {
                        id: id(),
                        batch_id: b.batch_id,
                        annotation_id: b.annotation_id,
                        started_at: b.at,
                        ended_at: Some(now()),
                        state: TranscriptSegmentState::Transcribing,
                        original_text: String::new(),
                    },
                    transcript: Box::pin(async {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        Ok("Bitte diesen Bereich verbessern.".into())
                    }),
                })
                .collect())
        })
    }
    fn cancel(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.boundary.lock().map_err(lock_error)?.take();
            Ok(())
        })
    }
}
#[derive(Default)]
pub struct FakeBrowser {
    sessions: Mutex<BTreeMap<Id, BrowserSession>>,
}
impl BrowserController for FakeBrowser {
    fn start_session(&self, mut s: BrowserSession) -> AdapterFuture<'_, BrowserSession> {
        Box::pin(async move {
            s.state = BrowserSessionState::Running;
            self.sessions
                .lock()
                .map_err(lock_error)?
                .insert(s.id.clone(), s.clone());
            Ok(s)
        })
    }
    fn stop_session(&self, id: Id) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.sessions.lock().map_err(lock_error)?.remove(&id);
            Ok(())
        })
    }
    fn duplicate_session(
        &self,
        mut s: BrowserSession,
        new: Id,
    ) -> AdapterFuture<'_, BrowserSession> {
        Box::pin(async move {
            s.id = new.clone();
            s.mode = SessionMode::Isolated;
            s.control.release();
            for t in &mut s.tabs {
                t.id = id();
                t.session_id = new.clone();
            }
            s.created_at = now();
            self.sessions
                .lock()
                .map_err(lock_error)?
                .insert(new, s.clone());
            Ok(s)
        })
    }
    fn open_tab(&self, session: Id, url: String) -> AdapterFuture<'_, BrowserTab> {
        Box::pin(async move {
            let t = BrowserTab {
                id: id(),
                session_id: session.clone(),
                url,
                title: "Demo".into(),
                active: true,
                scriptable: true,
            };
            if let Some(s) = self.sessions.lock().map_err(lock_error)?.get_mut(&session) {
                for t in &mut s.tabs {
                    t.active = false;
                }
                s.tabs.push(t.clone());
            }
            Ok(t)
        })
    }
    fn list_tabs(&self, s: Id) -> AdapterFuture<'_, Vec<BrowserTab>> {
        Box::pin(async move {
            Ok(self
                .sessions
                .lock()
                .map_err(lock_error)?
                .get(&s)
                .map(|s| s.tabs.clone())
                .unwrap_or_default())
        })
    }
    fn capture(&self, r: CaptureRequest) -> AdapterFuture<'_, CaptureResult> {
        Box::pin(async move {
            let tabs = self.list_tabs(r.session_id.clone()).await?;
            let tab = tabs
                .into_iter()
                .find(|t| r.tab_id.as_ref().map_or(t.active, |id| &t.id == id))
                .unwrap_or(BrowserTab {
                    id: id(),
                    session_id: r.session_id,
                    url: "http://localhost:3000/demo".into(),
                    title: "Demo".into(),
                    active: true,
                    scriptable: true,
                });
            let mut bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .map_err(|e| AdapterError::Io(e.to_string()))?
                    .write_image_data(&vec![180; 64 * 64 * 3])
                    .map_err(|e| AdapterError::Io(e.to_string()))?;
            }
            Ok(CaptureResult {
                images: vec![CaptureImage {
                    kind: AssetKind::ScreenshotMarked,
                    bytes,
                    width: 64,
                    height: 64,
                }],
                tab,
                viewport: Some(AnnotationViewport {
                    width: 64.,
                    height: 64.,
                    device_scale_factor: 1.,
                }),
                scroll: None,
                cursor: None,
                rect: r.rect,
                dom: if r.kind == AnnotationType::Dom {
                    Some(DomCapture {
                        locators: vec![LocatorCandidate {
                            kind: LocatorCandidateKind::Testid,
                            value: "save".into(),
                            playwright: Some("getByTestId('save')".into()),
                            stable: true,
                            reason: None,
                        }],
                        tag: "button".into(),
                        role: Some("button".into()),
                        name: Some("Speichern".into()),
                        text: Some("Speichern".into()),
                        attributes: BTreeMap::new(),
                        bounding_box: Rect {
                            x: 0.,
                            y: 0.,
                            width: 64.,
                            height: 64.,
                        },
                        frame_path: vec![],
                        shadow_path: vec![],
                        parent: None,
                        children: vec![],
                    })
                } else {
                    None
                },
            })
        })
    }
    fn set_review_mode(&self, _: bool) -> AdapterFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn push_bindings(&self, _: Vec<Binding>) -> AdapterFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn control(&self, s: Id, r: ControlRequest) -> AdapterFuture<'_, ControlState> {
        Box::pin(async move {
            let mut sessions = self.sessions.lock().map_err(lock_error)?;
            let s = sessions
                .get_mut(&s)
                .ok_or_else(|| AdapterError::Rejected("session absent".into()))?;
            match r.action {
                ControlRequestAction::Takeover | ControlRequestAction::Revoke => {
                    s.control.take_over(false)
                }
                ControlRequestAction::Release => s.control.release(),
                ControlRequestAction::Grant => {
                    if !s.agent_access {
                        return Err(AdapterError::Rejected("agent access disabled".into()));
                    }
                    s.control
                        .grant(
                            r.agent_id.unwrap_or_default(),
                            (time::OffsetDateTime::now_utc()
                                + time::Duration::seconds(
                                    r.lease_seconds.unwrap_or(60.).clamp(1., 3600.) as i64,
                                ))
                            .format(&time::format_description::well_known::Rfc3339)
                            .map_err(|e| AdapterError::Rejected(e.to_string()))?,
                        )
                        .map_err(|e| AdapterError::Rejected(e.to_string()))?;
                }
            }
            Ok(s.control.clone())
        })
    }
}
pub struct NoopHotkeys;
impl DesktopHotkeys for NoopHotkeys {
    fn apply(&self, _: Vec<Binding>) -> AdapterFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn check_conflicts(&self, _: Vec<Binding>) -> AdapterFuture<'_, Vec<BindingIssue>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn clear(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
