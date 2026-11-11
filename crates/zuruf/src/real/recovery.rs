//! One owner per session serializes start, recovery, restart and stop.
//! An intent revision cancels in-flight launches and makes the newest request win.
use super::*;
use tokio::{sync::watch, task::JoinHandle};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Intent {
    revision: u64,
    running: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Connecting,
    Ready,
    Retrying,
    Stopped,
}

#[derive(Clone, Copy)]
struct Progress {
    revision: u64,
    phase: Phase,
}

pub(super) struct SessionLife {
    intent: watch::Sender<Intent>,
    progress: watch::Sender<Progress>,
    task: AsyncMutex<Option<JoinHandle<()>>>,
}

struct LaunchedSession {
    browser: Arc<Browser>,
    model: BrowserSession,
    events: tokio::sync::broadcast::Receiver<zuruf_browser::BrowserEvent>,
}

impl SessionLife {
    fn new() -> Self {
        Self {
            intent: watch::channel(Intent::default()).0,
            progress: watch::channel(Progress {
                revision: 0,
                phase: Phase::Stopped,
            })
            .0,
            task: AsyncMutex::default(),
        }
    }

    fn request(&self, running: bool, force: bool) -> Intent {
        let mut requested = Intent::default();
        self.intent.send_if_modified(|current| {
            let changed = force || current.running != running;
            if changed {
                current.revision = current.revision.wrapping_add(1);
                current.running = running;
            }
            requested = *current;
            changed
        });
        requested
    }

    async fn wait(&self, requested: Intent) -> std::result::Result<(), AdapterError> {
        let mut progress = self.progress.subscribe();
        progress
            .wait_for(|p| {
                p.revision > requested.revision
                    || (p.revision == requested.revision && p.phase != Phase::Connecting)
            })
            .await
            .map_err(io)?;
        Ok(())
    }

    fn report(&self, intent: Intent, phase: Phase) {
        self.progress.send_replace(Progress {
            revision: intent.revision,
            phase,
        });
    }
}

fn retry_delay(failures: u32) -> Duration {
    Duration::from_millis(match failures {
        0 | 1 => 250,
        2 => 500,
        3 => 1_000,
        4 => 2_000,
        5 => 5_000,
        6 => 10_000,
        _ => 30_000,
    })
}

impl Browsers {
    pub(super) fn daemon(&self) -> std::result::Result<Daemon, AdapterError> {
        self.app
            .get()
            .and_then(std::sync::Weak::upgrade)
            .map(Daemon)
            .ok_or_else(|| AdapterError::Unavailable("Anwendung wird beendet".into()))
    }

    async fn model(&self, session: &str) -> std::result::Result<BrowserSession, AdapterError> {
        self.daemon()?
            .snapshot()
            .await
            .map_err(io)?
            .sessions
            .into_iter()
            .find(|s| s.id == session)
            .ok_or_else(|| AdapterError::Rejected("Sitzung nicht gefunden".into()))
    }

    async fn lifecycle(
        self: &Arc<Self>,
        session: &str,
    ) -> std::result::Result<Arc<SessionLife>, AdapterError> {
        let mut sessions = self.lifecycles.lock().await;
        if *self.shutting_down.borrow() {
            return Err(AdapterError::Unavailable("Anwendung wird beendet".into()));
        }
        if let Some(existing) = sessions.get(session) {
            return Ok(existing.clone());
        }
        let life = Arc::new(SessionLife::new());
        let task = tokio::spawn(self.clone().supervise(session.to_owned(), life.clone()));
        *life.task.lock().await = Some(task);
        sessions.insert(session.to_owned(), life.clone());
        Ok(life)
    }

    pub(super) async fn request_session(
        self: Arc<Self>,
        session: BrowserSession,
        restart: bool,
    ) -> std::result::Result<BrowserSession, AdapterError> {
        let sid = session.id.clone();
        self.model(&sid).await?;
        let life = self.lifecycle(&sid).await?;
        let requested = life.request(true, restart);
        life.wait(requested).await?;
        self.model(&sid).await
    }

    pub(super) async fn stop_managed(
        self: Arc<Self>,
        session: &str,
    ) -> std::result::Result<(), AdapterError> {
        let carried_app = self.sessions.lock().await.contains_key(session);
        let life = self.lifecycle(session).await?;
        let requested = life.request(false, true);
        life.wait(requested).await?;
        if carried_app && *life.intent.borrow() == requested && !*self.shutting_down.borrow() {
            // Closing the session also removes its docked panel. Keep the application
            // visible in another running session, or reopen its standalone window.
            self.open_app(&self.daemon()?, "sessions").await?;
        }
        Ok(())
    }

    pub(crate) fn cancel_recovery(&self) {
        self.shutting_down.send_replace(true);
    }

    pub(crate) async fn shutdown(&self) {
        self.cancel_recovery();
        let lives: Vec<_> = self.lifecycles.lock().await.values().cloned().collect();
        for life in lives {
            let task = life.task.lock().await.take();
            if let Some(task) = task {
                let _ = task.await;
            }
        }
        self.close_app().await;
    }

    async fn phase(
        &self,
        sid: &str,
        life: &Arc<SessionLife>,
        intent: Intent,
        state: BrowserSessionState,
        replacement: Option<BrowserSession>,
    ) -> std::result::Result<(), AdapterError> {
        let sid = sid.to_owned();
        let life = life.clone();
        self.daemon()?
            .mutate(move |s, _| {
                if *life.intent.borrow() != intent {
                    return Ok(());
                }
                if let Some(current) = s.sessions.iter_mut().find(|s| s.id == sid) {
                    let human = current.control.owner == ControlStateOwner::Human;
                    if let Some(updated) = replacement {
                        *current = updated;
                    }
                    current.state = state;
                    current.control = ControlState {
                        owner: if human {
                            ControlStateOwner::Human
                        } else {
                            ControlStateOwner::None
                        },
                        agent_id: None,
                        lease_expires_at: None,
                        settling: false,
                    };
                }
                Ok(())
            })
            .await
            .map_err(io)
    }

    async fn withdraw_access(&self, session: &str) {
        if let Ok(app) = self.daemon()
            && let Some(runtime) = app.0.real.get()
            && let Some(access) = runtime.access.lock().await.remove(session)
        {
            access.revoke();
        }
    }

    async fn supervise(self: Arc<Self>, session: String, life: Arc<SessionLife>) {
        let mut wanted = life.intent.subscribe();
        let mut shutdown = self.shutting_down.subscribe();
        let mut failures: u32 = 0;
        loop {
            if *shutdown.borrow() {
                break;
            }
            let intent = *wanted.borrow_and_update();
            if !intent.running {
                if intent.revision > 0 {
                    let _ = self
                        .phase(&session, &life, intent, BrowserSessionState::Stopped, None)
                        .await;
                }
                life.report(intent, Phase::Stopped);
                tokio::select! {
                    _ = wanted.changed() => {},
                    _ = shutdown.changed() => break,
                }
                failures = 0;
                continue;
            }
            let _ = self
                .phase(&session, &life, intent, BrowserSessionState::Starting, None)
                .await;
            life.report(intent, Phase::Connecting);
            let launched = tokio::select! {
                result = self.launch(&session) => Some(result),
                _ = wanted.changed() => None,
                _ = shutdown.changed() => break,
            };
            let Some(launched) = launched else {
                continue;
            };
            match launched {
                Ok(LaunchedSession {
                    browser,
                    model,
                    events,
                }) => {
                    if *wanted.borrow() != intent || *shutdown.borrow() {
                        let _ = browser.stop().await;
                        continue;
                    }
                    self.sessions
                        .lock()
                        .await
                        .insert(session.clone(), browser.clone());
                    let _ = self
                        .phase(
                            &session,
                            &life,
                            intent,
                            BrowserSessionState::Running,
                            Some(model),
                        )
                        .await;
                    life.report(intent, Phase::Ready);
                    let up_since = Instant::now();
                    tokio::select! {
                        _ = self.clone().listen(session.clone(), events) => {},
                        _ = async {
                            loop {
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                if browser.health().await.is_err() { break; }
                            }
                        } => {},
                        _ = wanted.changed() => {},
                        _ = shutdown.changed() => {},
                    }
                    self.sessions.lock().await.remove(&session);
                    self.withdraw_access(&session).await;
                    let _ = browser.stop().await;
                    if up_since.elapsed() >= Duration::from_secs(30) {
                        failures = 0;
                    }
                }
                Err(error) => {
                    // Keep the UI in a neutral reconnecting state; no repeated notices.
                    tracing::debug!(session_id=%session, error=%error, "browser launch will be retried");
                }
            }
            if *shutdown.borrow() {
                break;
            }
            if *wanted.borrow() != intent {
                continue;
            }
            failures = failures.saturating_add(1);
            let _ = self
                .phase(&session, &life, intent, BrowserSessionState::Starting, None)
                .await;
            life.report(intent, Phase::Retrying);
            tokio::select! {
                _ = tokio::time::sleep(retry_delay(failures)) => {},
                _ = wanted.changed() => {},
                _ = shutdown.changed() => break,
            }
        }
        // Graceful daemon shutdown preserves the on-disk running intent for the next boot.
        let intent = *wanted.borrow();
        life.report(intent, Phase::Stopped);
    }

    async fn launch(&self, sid: &str) -> std::result::Result<LaunchedSession, AdapterError> {
        let mut session = self.model(sid).await?;
        let daemon = self.daemon()?;
        let snapshot = daemon.snapshot().await.map_err(io)?;
        let mut config = SessionConfig::new(self.paths.data.join("sessions").join(sid));
        config.session_id = sid.to_owned();
        config.theme = theme_name(snapshot.settings.ui.theme).into();
        let code = daemon.mint_code().await.map_err(io)?;
        config.panel_url = Some(format!(
            "http://127.0.0.1:{}/panel.html#code={code}",
            self.port
        ));
        config.urls = session.tabs.iter().map(|tab| tab.url.clone()).collect();
        if config.urls.is_empty() {
            config.urls.push(
                snapshot
                    .projects
                    .iter()
                    .find(|p| p.id == session.project_id)
                    .map(|p| p.base_url.clone())
                    .filter(|url| !url.is_empty())
                    .unwrap_or_else(|| "about:blank".into()),
            );
        }
        let browser = Arc::new(Browser::start(config).await.map_err(io)?);
        // A cached page can become scriptable while configuration and the initial tab
        // snapshot are prepared. Subscribe first so that readiness cannot fall in a gap.
        let events = browser.events();
        let bindings = self.bindings.lock().await.clone();
        browser
            .push_bindings(convert(bindings.clone())?)
            .await
            .map_err(io)?;
        browser
            .set_review_mode(*self.review.lock().await, convert(bindings)?)
            .await
            .map_err(io)?;
        if session.control.owner == ControlStateOwner::Human {
            browser.takeover().await.map_err(io)?;
        }
        let width = self.widths.lock().await.get(sid).copied().flatten();
        browser.set_page_width(width).await.map_err(io)?;
        let mut mapped = Vec::new();
        for tab in browser.list_tabs().await.map_err(io)? {
            mapped.push(self.tab(tab).await?);
        }
        session.tabs = mapped;
        if snapshot.settings.ui.browser == SettingsUiBrowser::Integrated {
            self.close_app().await;
        }
        Ok(LaunchedSession {
            browser,
            model: session,
            events,
        })
    }

    pub(super) async fn set_page_width(
        &self,
        session: &str,
        width: Option<u32>,
    ) -> std::result::Result<(), AdapterError> {
        self.model(session).await?;
        let mut widths = self.widths.lock().await;
        let mut updated = widths.clone();
        updated.insert(session.to_owned(), width);
        let bytes = serde_json::to_vec(&updated).map_err(io)?;
        let path = self.paths.state.join("page-widths.json");
        tokio::task::spawn_blocking(move || zuruf_store::atomic_write(&path, &bytes))
            .await
            .map_err(io)?
            .map_err(io)?;
        *widths = updated;
        if let Some(browser) = self.sessions.lock().await.get(session).cloned() {
            browser.set_page_width(width).await.map_err(io)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_start_is_idempotent_and_stop_invalidates_recovery() {
        let life = SessionLife::new();
        let started = life.request(true, false);
        assert_eq!(life.request(true, false).revision, started.revision);
        let stopped = life.request(false, true);
        assert!(!stopped.running);
        assert!(stopped.revision > started.revision);
        let next = life.request(true, false);
        assert!(next.revision > stopped.revision);
        assert!(life.request(true, true).revision > next.revision);
    }

    #[test]
    fn retry_backoff_is_bounded_and_gives_a_persistent_failure_time_to_recover() {
        assert!(retry_delay(1) < retry_delay(4));
        assert_eq!(retry_delay(1000), Duration::from_secs(30));
    }
}
