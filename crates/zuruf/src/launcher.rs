//! Start options: the things a project can open.
//!
//! A start option names an address and may carry a command line (a dev server, say). Starting it
//! runs the command, waits until the address answers and opens it in a browser session that
//! belongs to the start option. Commands are only ever written through the local app's API;
//! the agent-facing MCP server has no tool that creates, changes or starts one.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// A command that is running for a start option. Its watcher task owns the child process.
pub(crate) struct Running {
    pid: u32,
    /// Set before Zuruf ends the process itself, so the exit is not reported as a failure.
    stopping: Arc<AtomicBool>,
}

/// How long a freshly started command may take until its address answers.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const READY_POLL: Duration = Duration::from_millis(400);
/// How long a command gets to end on its own before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
const SMALLEST_MEMORY_LIMIT_MB: f64 = 64.;

const EDITABLE: &[&str] = &[
    "name",
    "url",
    "repositoryPath",
    "command",
    "workingDir",
    "memoryLimitMb",
    "browser",
];

fn existing_folder(path: &str, what: &str) -> Result<String> {
    let folder =
        std::fs::canonicalize(path).map_err(|_| invalid(&format!("{what} existiert nicht.")))?;
    if !folder.is_dir() {
        return Err(invalid(&format!("{what} muss ein Verzeichnis sein.")));
    }
    Ok(folder.to_string_lossy().into_owned())
}

fn filled(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Normalise what the app sent and refuse what could not be started later.
fn checked(mut launcher: Launcher) -> Result<Launcher> {
    launcher.name = launcher.name.trim().to_owned();
    if launcher.name.is_empty() {
        return Err(invalid("Die Startoption braucht einen Namen."));
    }
    launcher.url = launcher.url.trim().to_owned();
    let address = reqwest::Url::parse(&launcher.url).ok();
    if !address.is_some_and(|url| matches!(url.scheme(), "http" | "https")) {
        return Err(invalid(
            "Die Adresse muss mit http:// oder https:// beginnen.",
        ));
    }
    launcher.repository_path = filled(launcher.repository_path)
        .map(|path| existing_folder(&path, "Der Ordner"))
        .transpose()?;
    launcher.working_dir = filled(launcher.working_dir)
        .map(|path| existing_folder(&path, "Der Ordner für den Startbefehl"))
        .transpose()?;
    launcher.command = filled(launcher.command);
    if launcher
        .memory_limit_mb
        .is_some_and(|megabytes| megabytes < SMALLEST_MEMORY_LIMIT_MB)
    {
        return Err(invalid("Das Speicherlimit muss mindestens 64 MB betragen."));
    }
    Ok(launcher)
}

fn find(s: &Snapshot, id: &str) -> Result<Launcher> {
    s.launchers
        .iter()
        .find(|launcher| launcher.id == id)
        .cloned()
        .ok_or_else(missing)
}

impl Daemon {
    pub(crate) async fn launcher_api(
        &self,
        method: &str,
        key: String,
        operation: &str,
        body: Value,
    ) -> Result<Value> {
        match (method, key.as_str(), operation) {
            ("GET", "", _) => val(self.snapshot().await?.launchers),
            ("POST", "", _) => self.create_launcher(body).await,
            ("POST", _, "start") => self.start_launcher(key).await,
            ("POST", _, "stop") => self.stop_launcher(key).await,
            ("PATCH", _, "") => self.patch_launcher(key, body).await,
            ("DELETE", _, "") => self.remove_launcher(key).await,
            _ => Err(missing()),
        }
    }

    async fn create_launcher(&self, body: Value) -> Result<Value> {
        self.mutate(move |s, _| {
            let mut item = body;
            if !item.is_object() {
                return Err(invalid("object required"));
            }
            item["id"] = json!(id());
            item["process"] = val(LauncherProcess::default())?;
            item["createdAt"] = json!(now());
            item["updatedAt"] = json!(now());
            let launcher = checked(serde_json::from_value(item)?)?;
            if !s.projects.iter().any(|p| p.id == launcher.project_id) {
                return Err(missing());
            }
            s.launchers.push(launcher.clone());
            val(launcher)
        })
        .await
    }

    async fn patch_launcher(&self, key: String, body: Value) -> Result<Value> {
        self.mutate(move |s, _| {
            let mut changed = find(s, &key)?;
            patch(&mut changed, body, EDITABLE)?;
            let mut changed = checked(changed)?;
            changed.updated_at = now();
            let slot = s
                .launchers
                .iter_mut()
                .find(|launcher| launcher.id == key)
                .ok_or_else(missing)?;
            *slot = changed.clone();
            val(changed)
        })
        .await
    }

    async fn remove_launcher(&self, key: String) -> Result<Value> {
        find(&self.snapshot().await?, &key)?;
        self.stop_launcher(key.clone()).await?;
        self.mutate(move |s, _| {
            let removed = find(s, &key)?;
            s.launchers.retain(|launcher| launcher.id != key);
            // Its sessions stay as ordinary sessions of the project.
            for session in &mut s.sessions {
                if session.launcher_id.as_ref() == Some(&key) {
                    session.launcher_id = None;
                }
            }
            val(removed)
        })
        .await
    }

    async fn start_launcher(&self, key: String) -> Result<Value> {
        let snapshot = self.snapshot().await?;
        let launcher = find(&snapshot, &key)?;
        let Some(command) = launcher.command.clone() else {
            self.open_launcher(key.clone()).await?;
            return val(find(&self.snapshot().await?, &key)?);
        };
        let already_running = self.0.launchers.lock().await.contains_key(&key);
        if already_running {
            // A command that is still coming up opens its address by itself.
            if launcher.process.state == LauncherProcessState::Running {
                self.open_launcher(key.clone()).await?;
            }
            return val(find(&self.snapshot().await?, &key)?);
        }
        self.spawn_command(&snapshot, &launcher, &command).await?;
        let app = self.clone();
        let waiting = key.clone();
        let job = tokio::spawn(async move {
            if !app.wait_until_ready(&waiting, &launcher.url).await {
                return;
            }
            if let Err(error) = app.open_launcher(waiting.clone()).await {
                app.launcher_notice(&waiting, &error.to_string());
            }
        });
        if let Ok(mut handles) = self.0.handles.lock() {
            handles.retain(|handle| !handle.is_finished());
            handles.push(job.abort_handle());
        }
        val(find(&self.snapshot().await?, &key)?)
    }

    fn launcher_notice(&self, key: &str, detail: &str) {
        let _ = self.0.events.send(json!({
            "type": "notice",
            "payload": {"level": "error", "code": "launcher.failed", "detail": detail, "relatedId": key},
        }));
    }

    async fn set_process(&self, key: &str, process: LauncherProcess) -> Result<()> {
        let key = key.to_owned();
        self.mutate(move |s, _| {
            if let Some(launcher) = s.launchers.iter_mut().find(|launcher| launcher.id == key) {
                launcher.process = process;
            }
            Ok(())
        })
        .await
    }

    /// Run the start option's command and watch it until it ends.
    async fn spawn_command(
        &self,
        snapshot: &Snapshot,
        launcher: &Launcher,
        command_line: &str,
    ) -> Result<()> {
        let project_folder = snapshot
            .projects
            .iter()
            .find(|project| project.id == launcher.project_id)
            .map(|project| project.repository_path.clone())
            .filter(|path| !path.is_empty());
        let folder = launcher
            .working_dir
            .clone()
            .or_else(|| launcher.repository_path.clone())
            .or(project_folder)
            .ok_or_else(|| invalid("Für den Startbefehl ist kein Ordner hinterlegt."))?;
        let key = launcher.id.clone();
        let log = self
            .access({
                let key = key.clone();
                move |p| {
                    let directory = p.store.paths.state.join("launchers");
                    zuruf_store::private_dir(&directory)?;
                    Ok(directory.join(format!("{key}.log")))
                }
            })
            .await?;
        let output = std::fs::File::create(&log).map_err(StoreError::from)?;
        zuruf_platform::private_file(&log).map_err(StoreError::from)?;
        let limit = launcher.memory_limit_mb.map(|megabytes| megabytes as u64);
        let mut command = zuruf_platform::shell(command_line, limit);
        command
            .current_dir(&folder)
            .stdin(std::process::Stdio::null())
            .stdout(output.try_clone().map_err(StoreError::from)?)
            .stderr(output);
        let mut child = tokio::process::Command::from(command)
            .spawn()
            .map_err(|error| {
                invalid(&format!("Der Startbefehl ließ sich nicht starten: {error}"))
            })?;
        let pid = child
            .id()
            .ok_or_else(|| invalid("Der Startbefehl endete sofort."))?;
        let stopping = Arc::new(AtomicBool::new(false));
        self.0.launchers.lock().await.insert(
            key.clone(),
            Running {
                pid,
                stopping: stopping.clone(),
            },
        );
        self.set_process(
            &key,
            LauncherProcess {
                state: LauncherProcessState::Starting,
                pid: Some(pid.into()),
                started_at: Some(now()),
                exit_code: None,
                error: None,
            },
        )
        .await?;
        let app = self.clone();
        tokio::spawn(async move {
            let exit = child.wait().await.ok();
            let mut running = app.0.launchers.lock().await;
            if running.get(&key).is_some_and(|entry| entry.pid == pid) {
                running.remove(&key);
            }
            drop(running);
            let code = exit.and_then(|status| status.code());
            let ended_as_asked = stopping.load(Ordering::SeqCst) || code == Some(0);
            let process = LauncherProcess {
                state: if ended_as_asked {
                    LauncherProcessState::Stopped
                } else {
                    LauncherProcessState::Failed
                },
                pid: None,
                started_at: None,
                exit_code: code.map(f64::from),
                error: (!ended_as_asked).then(|| {
                    let with_code = code
                        .map(|code| format!(" mit Code {code}"))
                        .unwrap_or_default();
                    format!(
                        "Der Startbefehl endete{with_code}. Protokoll: {}",
                        log.display()
                    )
                }),
            };
            let _ = app.set_process(&key, process).await;
        });
        Ok(())
    }

    /// True once the address accepts connections. False when the command ended first
    /// (its watcher reports that) or the address never answered.
    async fn wait_until_ready(&self, key: &str, address: &str) -> bool {
        let target = reqwest::Url::parse(address)
            .ok()
            .and_then(|url| Some((url.host_str()?.to_owned(), url.port_or_known_default()?)));
        let Some((host, port)) = target else {
            return false;
        };
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let Some(pid) = self
                .0
                .launchers
                .lock()
                .await
                .get(key)
                .map(|entry| entry.pid)
            else {
                return false;
            };
            let attempt = tokio::net::TcpStream::connect((host.as_str(), port));
            let answered = tokio::time::timeout(Duration::from_secs(2), attempt)
                .await
                .is_ok_and(|connection| connection.is_ok());
            let timed_out = Instant::now() > deadline;
            if answered || timed_out {
                let key = key.to_owned();
                let _ = self
                    .mutate(move |s, _| {
                        let process = s
                            .launchers
                            .iter_mut()
                            .find(|launcher| launcher.id == key)
                            .map(|launcher| &mut launcher.process)
                            .filter(|process| process.pid == Some(pid.into()));
                        if let Some(process) = process {
                            process.state = LauncherProcessState::Running;
                            process.error = timed_out.then(|| {
                                "Der Startbefehl läuft, aber die Adresse antwortet nicht.".into()
                            });
                        }
                        Ok(())
                    })
                    .await;
                return answered;
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }

    /// Bring the start option's address to the front in a session of its own.
    async fn open_launcher(&self, key: String) -> Result<()> {
        let snapshot = self.snapshot().await?;
        let launcher = find(&snapshot, &key)?;
        if snapshot.status.active_project_id.as_ref() != Some(&launcher.project_id) {
            self.set_review(false).await?;
        }
        let session = self
            .mutate({
                let launcher = launcher.clone();
                move |s, _| {
                    if s.status.active_project_id.as_ref() != Some(&launcher.project_id) {
                        activate_project(s, Some(launcher.project_id.clone()))?;
                    }
                    let existing = s
                        .sessions
                        .iter()
                        .position(|session| session.launcher_id.as_ref() == Some(&launcher.id));
                    let session = match existing {
                        Some(index) => s.sessions.remove(index),
                        None => {
                            let options =
                                browser_options(s, &launcher.project_id, Some(&launcher.id));
                            let mode = options.mode.unwrap_or(SessionMode::Isolated);
                            BrowserSession {
                                id: id(),
                                project_id: launcher.project_id.clone(),
                                name: launcher.name.clone(),
                                mode,
                                state: BrowserSessionState::Stopped,
                                agent_access: options
                                    .agent_access
                                    .unwrap_or(mode == SessionMode::Shared),
                                control: ControlState {
                                    owner: ControlStateOwner::None,
                                    agent_id: None,
                                    lease_expires_at: None,
                                    settling: false,
                                },
                                tabs: vec![],
                                launcher_id: Some(launcher.id.clone()),
                                created_at: now(),
                            }
                        }
                    };
                    // The session in front is the last one, as with any other switch.
                    s.sessions.push(session.clone());
                    s.status.active_session_id = Some(session.id.clone());
                    Ok(session)
                }
            })
            .await?;
        let open = matches!(
            session.state,
            BrowserSessionState::Running | BrowserSessionState::Starting
        );
        if !open {
            let started = self.0.browser.start_session(session).await?;
            return self.commit_session(started).await;
        }
        let address = launcher.url.trim_end_matches('/');
        if session.tabs.iter().any(|tab| tab.url.starts_with(address)) {
            return Ok(());
        }
        let tab = self
            .0
            .browser
            .open_tab(session.id.clone(), launcher.url.clone())
            .await?;
        let mut session = session;
        for other in &mut session.tabs {
            other.active = false;
        }
        session.tabs.push(tab);
        self.commit_session(session).await
    }

    /// The real browser runtime writes session changes itself; only the fake needs help.
    async fn commit_session(&self, session: BrowserSession) -> Result<()> {
        if self.0.real.get().is_some() {
            return Ok(());
        }
        self.mutate(move |s, _| {
            if let Some(slot) = s.sessions.iter_mut().find(|other| other.id == session.id) {
                *slot = session;
            }
            Ok(())
        })
        .await
    }

    /// End the start option's command and the browser session that shows it.
    async fn stop_launcher(&self, key: String) -> Result<Value> {
        let snapshot = self.snapshot().await?;
        find(&snapshot, &key)?;
        let running = self
            .0
            .launchers
            .lock()
            .await
            .get(&key)
            .map(|entry| (entry.pid, entry.stopping.clone()));
        match running {
            Some((pid, stopping)) => {
                stopping.store(true, Ordering::SeqCst);
                let _ = tokio::task::spawn_blocking(move || zuruf_platform::stop_tree(pid, false))
                    .await;
                let app = self.clone();
                let watched = key.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(STOP_GRACE).await;
                    let still_there = app
                        .0
                        .launchers
                        .lock()
                        .await
                        .get(&watched)
                        .is_some_and(|entry| entry.pid == pid);
                    if still_there {
                        let _ = tokio::task::spawn_blocking(move || {
                            zuruf_platform::stop_tree(pid, true)
                        })
                        .await;
                    }
                });
            }
            // Nothing runs: clear what an earlier failure left behind.
            None => self.set_process(&key, LauncherProcess::default()).await?,
        }
        let open_sessions = snapshot.sessions.into_iter().filter(|session| {
            session.launcher_id.as_ref() == Some(&key)
                && session.state != BrowserSessionState::Stopped
        });
        for session in open_sessions {
            self.0.browser.stop_session(session.id.clone()).await?;
            self.commit_session(BrowserSession {
                state: BrowserSessionState::Stopped,
                ..session
            })
            .await?;
        }
        val(find(&self.snapshot().await?, &key)?)
    }

    /// End every running command. Used when a project goes away and when the daemon closes.
    pub(crate) async fn stop_launcher_commands(&self, of_project: Option<&str>) {
        let snapshot = match self.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) => return,
        };
        let mut pids = Vec::new();
        for (key, entry) in self.0.launchers.lock().await.iter() {
            let belongs = of_project.is_none_or(|project| {
                snapshot
                    .launchers
                    .iter()
                    .any(|launcher| &launcher.id == key && launcher.project_id == project)
            });
            if belongs {
                entry.stopping.store(true, Ordering::SeqCst);
                pids.push(entry.pid);
            }
        }
        if pids.is_empty() {
            return;
        }
        for pid in pids.clone() {
            let _ =
                tokio::task::spawn_blocking(move || zuruf_platform::stop_tree(pid, false)).await;
        }
        let deadline = Instant::now() + STOP_GRACE;
        loop {
            let alive: Vec<u32> = self
                .0
                .launchers
                .lock()
                .await
                .values()
                .map(|entry| entry.pid)
                .filter(|pid| pids.contains(pid))
                .collect();
            if alive.is_empty() {
                return;
            }
            if Instant::now() > deadline {
                for pid in alive {
                    let _ =
                        tokio::task::spawn_blocking(move || zuruf_platform::stop_tree(pid, true))
                            .await;
                }
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
