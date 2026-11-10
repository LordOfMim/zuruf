use super::*;
impl Daemon {
    pub(crate) fn start_worker(&self, id: Id) {
        let Ok(mut running) = self.0.workers.lock() else {
            return;
        };
        if !running.insert(id.clone()) {
            return;
        }
        let app = self.clone();
        let job = tokio::spawn(async move {
            if let Err(e) = app.work(id.clone()).await {
                tracing::error!(task_id=%id,error=%e,"dispatch worker failed");
                let text = e.to_string();
                let key = id.clone();
                let _ = app
                    .mutate(move |s, _| {
                        if let Some(t) = s
                            .tasks
                            .iter_mut()
                            .find(|t| t.id == key && !t.status.terminal())
                        {
                            t.status = TaskStatus::Failed;
                            t.error = Some(text);
                        }
                        Ok(())
                    })
                    .await;
            }
            if let Ok(mut running) = app.0.workers.lock() {
                running.remove(&id);
            }
            // A retry can race with the final iteration of the previous worker.
            if let Ok(snapshot) = app.snapshot().await
                && snapshot.tasks.iter().any(|t| {
                    t.id == id
                        && matches!(
                            t.status,
                            TaskStatus::Queued
                                | TaskStatus::FinalizingTranscript
                                | TaskStatus::Dispatching
                                | TaskStatus::Running
                        )
                })
            {
                app.start_worker(id);
            }
        });
        if let Ok(mut handles) = self.0.handles.lock() {
            handles.retain(|h| !h.is_finished());
            handles.push(job.abort_handle());
        }
    }
    pub(crate) async fn apply_receipt(&self, id: Id, r: ThreadReceipt) -> Result<Value> {
        self.mutate(move |s, store| {
            store.save_receipt(&id, &r)?;
            store.clear_message(&id)?;
            let conflict = s.tasks.iter().any(|t| {
                t.id != id
                    && !t.status.terminal()
                    && t.workspace_path.is_some()
                    && t.workspace_path == r.workspace_path
            });
            let t = s
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(missing)?;
            let cancelled = t.status == TaskStatus::Cancelled;
            if !cancelled {
                t.status = TaskStatus::Running;
            }
            t.thread_id = Some(r.thread_id);
            t.thread_url = r.thread_url;
            t.workspace_path = r.workspace_path;
            t.branch = r.branch;
            t.workspace_conflict = conflict;
            t.dispatched_at = Some(now());
            t.error = None;
            t.pending_request = None;
            t.last_message = None;
            t.remote_status = None;
            val(t)
        })
        .await
    }
    async fn task_status(&self, id: Id, status: TaskStatus, error: Option<String>) -> Result<()> {
        if status.terminal() || status == TaskStatus::UnknownRemoteState {
            self.revoke_mcp_task(&id);
        }
        self.mutate(move |s, _| {
            let t = s
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(missing)?;
            if t.status == TaskStatus::Cancelled {
                return Ok(());
            }
            t.status = status;
            t.error = error.clone();
            if status.terminal() {
                t.finished_at = Some(now());
            }
            Ok(())
        })
        .await
    }
    async fn work(&self, id: Id) -> Result<()> {
        let finalize_started = Instant::now();
        loop {
            let s = self.snapshot().await?;
            let task = s
                .tasks
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or_else(missing)?;
            if task.status.terminal() || task.status == TaskStatus::UnknownRemoteState {
                self.revoke_mcp_task(&id);
                return Ok(());
            }
            if s.annotations.iter().any(|a| {
                a.batch_id == task.batch_id
                    && a.r#type != AnnotationType::Text
                    && !a
                        .assets
                        .iter()
                        .any(|asset| asset.kind == AssetKind::ScreenshotMarked)
            }) {
                if finalize_started.elapsed() > Duration::from_secs(120) {
                    self.task_status(
                        id,
                        TaskStatus::Failed,
                        Some("Capture did not finish; annotation retained".into()),
                    )
                    .await?;
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
                continue;
            }
            if task.status == TaskStatus::FinalizingTranscript {
                if s.segments.iter().any(|x| {
                    x.batch_id == task.batch_id
                        && matches!(
                            x.state,
                            TranscriptSegmentState::Recording
                                | TranscriptSegmentState::Transcribing
                        )
                }) {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    continue;
                }
                if s.segments.iter().any(|x| {
                    x.batch_id == task.batch_id && x.state == TranscriptSegmentState::Failed
                }) {
                    self.task_status(
                        id,
                        TaskStatus::Failed,
                        Some("Transkription fehlgeschlagen".into()),
                    )
                    .await?;
                    return Ok(());
                }
                self.task_status(id.clone(), TaskStatus::Queued, None)
                    .await?;
                continue;
            }
            let project = s
                .projects
                .iter()
                .find(|p| p.id == task.project_id)
                .cloned()
                .ok_or_else(missing)?;
            let receipt = self
                .access({
                    let id = id.clone();
                    move |p| Ok(p.store.receipt(&id)?)
                })
                .await?;
            if let Some(receipt) = receipt {
                if task.thread_id.is_none() {
                    self.apply_receipt(id.clone(), receipt.clone()).await?;
                }
                match self
                    .0
                    .t3
                    .thread_status(receipt.thread_id.clone(), receipt.run_id.clone())
                    .await
                {
                    Ok(remote) => {
                        if ["completed", "interrupted", "failed", "cancelled"]
                            .contains(&remote.status.as_str())
                        {
                            self.revoke_mcp_task(&id);
                            if let Some(session) = s
                                .sessions
                                .iter()
                                .find(|s| s.control.agent_id.as_deref() == Some(&id))
                            {
                                self.session_control(
                                    session.id.clone(),
                                    ControlRequest {
                                        action: ControlRequestAction::Revoke,
                                        agent_id: None,
                                        lease_seconds: None,
                                    },
                                )
                                .await?;
                            }
                        }
                        if !receipt.run_id.is_empty() && remote.run_id != receipt.run_id {
                            self.task_status(
                                id,
                                TaskStatus::UnknownRemoteState,
                                Some("Run mismatch".into()),
                            )
                            .await?;
                            return Ok(());
                        }
                        let status = map_remote_status(
                            &remote.status,
                            remote.approval,
                            remote.pending_request.is_some(),
                        );
                        let remote_error = remote.error.clone();
                        let key = id.clone();
                        self.mutate(move |s, _| {
                            let t = s
                                .tasks
                                .iter_mut()
                                .find(|t| t.id == key)
                                .ok_or_else(missing)?;
                            if t.status != TaskStatus::Cancelled {
                                t.remote_status = Some(remote.status);
                                t.last_message = remote.last_assistant_message;
                                t.pending_request = remote.pending_request;
                            }
                            Ok(())
                        })
                        .await?;
                        if task.status != status {
                            self.task_status(id.clone(), status, remote_error).await?;
                        }
                        if status.terminal() || status == TaskStatus::UnknownRemoteState {
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        self.task_status(id, TaskStatus::UnknownRemoteState, Some(e.to_string()))
                            .await?;
                        return Ok(());
                    }
                }
                tokio::time::sleep(Duration::from_secs_f64(
                    s.settings.t3.poll_seconds.clamp(0.05, 300.),
                ))
                .await;
                continue;
            }
            let task_key = id.clone();
            let message = self
                .access(move |p| Ok(p.store.message(&task_key)?))
                .await?;
            if task.status == TaskStatus::Dispatching && message.is_some() {
                self.task_status(
                    id,
                    TaskStatus::UnknownRemoteState,
                    Some("Follow-up receipt missing; resolve before resending".into()),
                )
                .await?;
                return Ok(());
            }
            // A dispatching task after restart may already have created a remote thread.
            if task.status == TaskStatus::Dispatching {
                let found = self
                    .0
                    .t3
                    .find_threads_by_marker(project.clone(), id.clone())
                    .await;
                match found {
                    Ok(found) if found.len() == 1 => {
                        self.apply_receipt(id.clone(), found[0].clone()).await?;
                        continue;
                    }
                    _ => {
                        self.task_status(
                            id,
                            TaskStatus::UnknownRemoteState,
                            Some("Remote outcome requires resolution".into()),
                        )
                        .await?;
                        return Ok(());
                    }
                }
            }
            let key = id.clone();
            let (manifest_path, prompt) = self
                .access(move |p| {
                    let s = &p.snapshot;
                    let task = s.tasks.iter().find(|t| t.id == key).ok_or_else(missing)?;
                    let batch = s
                        .batches
                        .iter()
                        .find(|b| b.id == task.batch_id)
                        .ok_or_else(missing)?;
                    let project = s
                        .projects
                        .iter()
                        .find(|p| p.id == task.project_id)
                        .ok_or_else(missing)?;
                    let annotations = s
                        .annotations
                        .iter()
                        .filter(|a| a.batch_id == batch.id)
                        .cloned()
                        .collect::<Vec<_>>();
                    let manifest = build_manifest(
                        batch,
                        project,
                        &task.profile,
                        &annotations,
                        &s.settings.redaction,
                    )
                    .map_err(|e| invalid(&e.to_string()))?;
                    let path = p.store.write_asset(
                        batch,
                        "batch.json",
                        &serde_json::to_vec_pretty(&manifest)?,
                    )?;
                    let path = path.to_string_lossy().into_owned();
                    let prompt = format!(
                        "{}\nBatch-Anweisung des Nutzers: {}",
                        build_prompt(&path).map_err(|e| invalid(&e.to_string()))?,
                        batch.note
                    );
                    Ok((path, prompt))
                })
                .await?;
            tracing::info!(task_id=%id,manifest=%manifest_path,"dispatching batch");
            self.task_status(id.clone(), TaskStatus::Dispatching, None)
                .await?;
            let mut prompt = message.as_ref().map(|m| m.prompt.clone()).unwrap_or(prompt);
            if let Some(path) = self.agent_credential_path(&task).await? {
                prompt.push_str(&format!("\nZuruf task MCP credential file: {path}. Read this private file locally without printing it. Use its url and Origin/Authorization headers for JSON-RPC tools/call. Never include its token in messages, logs or files. Available tools include get_batch_manifest, get_annotation_asset and report_progress. The credential expires and is revoked at task end."));
            }
            let send_id = message
                .as_ref()
                .map(|m| m.request_id.clone())
                .unwrap_or_else(|| id.clone());
            let request = CreateThreadRequest {
                client_request_id: id.clone(),
                profile: task.profile,
                project: project.clone(),
                title: format!("{} [zuruf:{}]", task.title, id),
                prompt,
            };
            if self
                .snapshot()
                .await?
                .tasks
                .iter()
                .any(|t| t.id == id && t.status == TaskStatus::Cancelled)
            {
                return Ok(());
            }
            let sending_follow_up = task.thread_id.is_some();
            let operation = async {
                let batch_id = task.batch_id.clone();
                let assets = if self.0.real.get().is_some() {
                    self.access(move |p| {
                        let mut assets = Vec::new();
                        for annotation in p
                            .snapshot
                            .annotations
                            .iter()
                            .filter(|a| a.batch_id == batch_id)
                        {
                            for asset in annotation
                                .assets
                                .iter()
                                .filter(|a| a.kind == AssetKind::ScreenshotMarked)
                            {
                                let batch = p
                                    .snapshot
                                    .batches
                                    .iter()
                                    .find(|b| b.id == batch_id)
                                    .ok_or_else(missing)?;
                                let path = zuruf_store::safe_join(
                                    &p.store.paths.batch_dir(batch)?,
                                    &asset.path,
                                )?;
                                let bytes = std::fs::read(path).map_err(StoreError::from)?;
                                assets.push((format!("{}.png", annotation.id), bytes));
                            }
                        }
                        Ok(assets)
                    })
                    .await
                    .map_err(|e| AdapterError::Io(e.to_string()))?
                } else {
                    vec![]
                };
                if let Some(thread) = task.thread_id {
                    self.0
                        .t3
                        .send_message_with_assets(thread, send_id, request.prompt, assets)
                        .await
                } else {
                    self.0.t3.create_thread_with_assets(request, assets).await
                }
            };
            let outcome = tokio::time::timeout(Duration::from_secs(30), operation)
                .await
                .unwrap_or_else(|_| {
                    Err(AdapterError::Ambiguous(
                        "T3 launch receipt timed out".into(),
                    ))
                });
            match outcome {
                Ok(receipt) => {
                    let thread = receipt.thread_id.clone();
                    let updated = self.apply_receipt(id.clone(), receipt).await?;
                    if updated["status"] == "cancelled" {
                        self.0.t3.interrupt(thread).await?;
                        return Ok(());
                    }
                }
                Err(AdapterError::Ambiguous(reason)) if sending_follow_up => {
                    self.task_status(id, TaskStatus::UnknownRemoteState, Some(reason))
                        .await?;
                    return Ok(());
                }
                Err(AdapterError::Ambiguous(reason)) => {
                    match self.0.t3.find_threads_by_marker(project, id.clone()).await {
                        Ok(found) if found.len() == 1 => {
                            self.apply_receipt(id.clone(), found[0].clone()).await?;
                        }
                        _ => {
                            self.task_status(id, TaskStatus::UnknownRemoteState, Some(reason))
                                .await?;
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    self.task_status(id, TaskStatus::Failed, Some(e.to_string()))
                        .await?;
                    return Ok(());
                }
            }
        }
    }
}
