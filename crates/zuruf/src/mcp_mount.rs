//! Minimal daemon bridge for the independent MCP crate.
use super::*;
use zuruf_mcp::{ApiFuture, Handoff, Scope, ZurufApi};

impl Daemon {
    /// Dispatch hands this result to a private per-thread MCP credential channel.
    pub async fn mcp_handoff(&self, scope: Scope, port: u16) -> Result<Handoff> {
        let s = self.snapshot().await?;
        let task = s
            .tasks
            .iter()
            .find(|t| t.id == scope.task_id && t.project_id == scope.project_id)
            .ok_or_else(missing)?;
        if !s
            .sessions
            .iter()
            .any(|s| s.id == scope.session_id && s.project_id == scope.project_id)
            || s.annotations
                .iter()
                .filter(|a| a.batch_id == task.batch_id)
                .any(|a| {
                    a.session_id
                        .as_ref()
                        .is_some_and(|id| id != &scope.session_id)
                })
        {
            return Err(missing());
        }
        self.0
            .agent_auth
            .issue(scope, port, Duration::from_secs(3600))
            .map_err(|e| Error::Join(e.to_string()))
    }
    /// The external T3 launch contract has no private per-thread MCP/env field.
    /// Hand off only a path; the secret remains in a private state file.
    pub(crate) async fn agent_credential_path(&self, task: &Task) -> Result<Option<String>> {
        let snapshot = self.snapshot().await?;
        let session = snapshot
            .annotations
            .iter()
            .find(|a| a.batch_id == task.batch_id && a.session_id.is_some())
            .and_then(|a| a.session_id.clone());
        let Some(session_id) = session else {
            return Ok(None);
        };
        let port = std::env::var("ZURUF_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(47600);
        let handoff = self
            .mcp_handoff(
                Scope {
                    project_id: task.project_id.clone(),
                    task_id: task.id.clone(),
                    session_id,
                },
                port,
            )
            .await?;
        let task_id = task.id.clone();
        self.access(move |p| {
            let path = p
                .store
                .paths
                .state
                .join("agents")
                .join(task_id)
                .join("credential.json");
            zuruf_store::atomic_write(
                &path,
                &serde_json::to_vec(&json!({
                    "url": handoff.url, "origin": handoff.origin, "token": handoff.token
                }))?,
            )?;
            Ok(Some(path.to_string_lossy().into_owned()))
        })
        .await
    }
    /// CDP wiring must validate the same credential registry before every command.
    pub fn mcp_agent_scope(&self, token: &str) -> Option<Scope> {
        self.0.agent_auth.scope(token)
    }
    pub fn revoke_mcp_task(&self, task: &str) {
        self.0.agent_auth.revoke_task(task);
        if let Ok(p) = self.0.persist.lock() {
            let _ = std::fs::remove_file(
                p.store
                    .paths
                    .state
                    .join("agents")
                    .join(task)
                    .join("credential.json"),
            );
        }
    }
}
pub(super) fn router(app: &Daemon) -> Router {
    zuruf_mcp::router(Arc::new(app.clone()), app.0.agent_auth.clone()).merge(
        Router::new()
            .route("/api/v1/mcp/credentials", axum::routing::post(credentials))
            .with_state(app.clone()),
    )
}
impl ZurufApi for Daemon {
    fn snapshot(&self) -> ApiFuture<'_, Snapshot> {
        Box::pin(async {
            self.snapshot()
                .await
                .map_err(|_| "snapshot unavailable".into())
        })
    }
    fn asset(&self, scope: Scope, asset: String) -> ApiFuture<'_, Vec<u8>> {
        Box::pin(async move {
            self.access(move |p| {
                let t = p
                    .snapshot
                    .tasks
                    .iter()
                    .find(|t| t.id == scope.task_id && t.project_id == scope.project_id)
                    .ok_or_else(missing)?;
                let a = p
                    .snapshot
                    .annotations
                    .iter()
                    .find(|a| {
                        a.batch_id == t.batch_id
                            && a.session_id
                                .as_deref()
                                .is_none_or(|id| id == scope.session_id)
                            && a.assets.iter().any(|x| x.id == asset)
                    })
                    .ok_or_else(missing)?;
                let asset = a
                    .assets
                    .iter()
                    .find(|x| x.id == asset && x.kind != AssetKind::Audio)
                    .ok_or_else(missing)?;
                let b = p
                    .snapshot
                    .batches
                    .iter()
                    .find(|b| b.id == t.batch_id)
                    .ok_or_else(missing)?;
                let path = zuruf_store::safe_join(&p.store.paths.batch_dir(b)?, &asset.path)?;
                let meta = std::fs::metadata(&path).map_err(StoreError::from)?;
                if meta.len() > 20 * 1024 * 1024 {
                    return Err(invalid("asset too large"));
                }
                Ok(std::fs::read(path).map_err(StoreError::from)?)
            })
            .await
            .map_err(|_| "asset unavailable".into())
        })
    }
    fn update(
        &self,
        scope: Scope,
        annotation: Option<String>,
        value: Value,
    ) -> ApiFuture<'_, Value> {
        Box::pin(async move {
            self.mutate(move |s, _| {
                let t = s
                    .tasks
                    .iter_mut()
                    .find(|t| t.id == scope.task_id && t.project_id == scope.project_id)
                    .ok_or_else(missing)?;
                if let Some(id) = annotation {
                    let a = s
                        .annotations
                        .iter_mut()
                        .find(|a| {
                            a.id == id
                                && a.batch_id == t.batch_id
                                && a.session_id
                                    .as_deref()
                                    .is_none_or(|id| id == scope.session_id)
                        })
                        .ok_or_else(missing)?;
                    a.status = serde_json::from_value(value)?;
                    a.updated_at = now();
                } else {
                    t.last_message = Some(
                        value["message"]
                            .as_str()
                            .ok_or_else(|| invalid("message"))?
                            .into(),
                    );
                    if value["needsReview"] == true {
                        t.status = t
                            .status
                            .transition(TaskStatus::NeedsReview)
                            .map_err(|_| invalid("transition"))?;
                    }
                }
                Ok(json!({"ok":true}))
            })
            .await
            .map_err(|_| "update rejected".into())
        })
    }
    fn control(&self, scope: Scope, action: String, seconds: u64) -> ApiFuture<'_, Value> {
        Box::pin(async move {
            // Serialize ownership checks with the daemon human-control route.
            let _guard = self.0.control_gate.lock().await;
            let s = self.snapshot().await.map_err(|_| "snapshot")?;
            let session = s
                .sessions
                .iter()
                .find(|s| {
                    s.id == scope.session_id && s.project_id == scope.project_id && s.agent_access
                })
                .ok_or("session")?;
            let mut control = session.control.clone();
            control.expire(&now());
            let owns = control.owner == ControlStateOwner::Agent
                && control.agent_id.as_deref() == Some(&scope.task_id);
            if action == "get_cdp_endpoint" {
                if !owns {
                    return Err("lease required".into());
                }
                let port = std::env::var("ZURUF_PORT").unwrap_or_else(|_| "47600".into());
                return Ok(
                    json!({"endpoint":format!("ws://127.0.0.1:{port}/cdp/{}",scope.session_id),"control":control}),
                );
            }
            let action = if action == "renew_browser_control" {
                if !owns {
                    return Err("not owner".into());
                }
                ControlRequestAction::Renew
            } else if action == "release_browser_control" {
                if !owns {
                    return Err("not owner".into());
                }
                ControlRequestAction::Release
            } else {
                if control.owner != ControlStateOwner::None {
                    return Err("control busy".into());
                }
                ControlRequestAction::Grant
            };
            let session_id = scope.session_id;
            let state = self
                .0
                .browser
                .control(
                    session_id.clone(),
                    ControlRequest {
                        action,
                        agent_id: Some(scope.task_id),
                        lease_seconds: Some(seconds as f64),
                    },
                )
                .await
                .map_err(|_| "control rejected")?;
            self.mutate(move |s, _| {
                let session = s
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session_id)
                    .ok_or_else(missing)?;
                session.control = state;
                Ok(json!({"ok":true}))
            })
            .await
            .map_err(|_| "control persistence failed".into())
        })
    }
}

/// Controller-only runtime credential handoff; never available to an agent token.
async fn credentials(
    State(app): State<Daemon>,
    headers: HeaderMap,
    axum::Json(value): axum::Json<Value>,
) -> Result<Response> {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if headers.get("origin").and_then(|o| o.to_str().ok())
        != Some(format!("http://{host}").as_str())
    {
        return Err(Error::Api(StatusCode::FORBIDDEN, "origin rejected".into()));
    }
    if headers.get("authorization").and_then(|h| h.to_str().ok())
        != Some(format!("Bearer {}", app.0.token).as_str())
    {
        return Err(Error::Api(
            StatusCode::UNAUTHORIZED,
            "authentication required".into(),
        ));
    }
    let scope: Scope = serde_json::from_value(value["scope"].clone())?;
    if value["revoke"] == true {
        app.revoke_mcp_task(&scope.task_id);
        return Ok(axum::Json(json!({"ok":true})).into_response());
    }
    let port = host
        .strip_prefix("127.0.0.1:")
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| invalid("loopback host required"))?;
    let h = app.mcp_handoff(scope, port).await?;
    Ok((
        [("cache-control", "no-store")],
        axum::Json(json!({"url":h.url,"origin":h.origin,"token":h.token})),
    )
        .into_response())
}
