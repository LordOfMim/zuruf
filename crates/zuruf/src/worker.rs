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
        });
        if let Ok(mut handles) = self.0.handles.lock() {
            handles.push(job.abort_handle());
        }
    }
    pub(crate) async fn apply_receipt(&self, id: Id, r: ThreadReceipt) -> Result<Value> {
        self.mutate(move |s, store| {
            store.save_receipt(&id, &r)?;
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
            if t.status == TaskStatus::Cancelled {
                return val(t);
            }
            t.status = TaskStatus::Running;
            t.thread_id = Some(r.thread_id);
            t.thread_url = r.thread_url;
            t.workspace_path = r.workspace_path;
            t.branch = r.branch;
            t.workspace_conflict = conflict;
            t.dispatched_at = Some(now());
            t.error = None;
            val(t)
        })
        .await
    }
    async fn task_status(&self, id: Id, status: TaskStatus, error: Option<String>) -> Result<()> {
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
        loop {
            let s = self.snapshot().await?;
            let task = s
                .tasks
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or_else(missing)?;
            if task.status.terminal() || task.status == TaskStatus::UnknownRemoteState {
                return Ok(());
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
                            self.task_status(id.clone(), status, None).await?;
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
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
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
                    let prompt = build_prompt(&path).map_err(|e| invalid(&e.to_string()))?;
                    Ok((path, prompt))
                })
                .await?;
            tracing::info!(task_id=%id,manifest=%manifest_path,"dispatching batch");
            self.task_status(id.clone(), TaskStatus::Dispatching, None)
                .await?;
            let request = CreateThreadRequest {
                client_request_id: id.clone(),
                profile: task.profile,
                project: project.clone(),
                title: format!("{} [zuruf:{}]", task.title, id),
                prompt,
            };
            let sending_follow_up = task.thread_id.is_some();
            let outcome = if let Some(thread) = task.thread_id {
                self.0
                    .t3
                    .send_message(thread, id.clone(), request.prompt)
                    .await
            } else {
                self.0.t3.create_thread(request).await
            };
            match outcome {
                Ok(receipt) => {
                    self.apply_receipt(id.clone(), receipt).await?;
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
