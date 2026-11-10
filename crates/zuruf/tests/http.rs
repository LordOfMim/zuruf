use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use zuruf::{Daemon, fakes::*};
use zuruf_core::*;
use zuruf_store::Paths;
struct Harness {
    app: Daemon,
    base: String,
    token: String,
    t3: Arc<FakeT3>,
    server: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
    paths: Paths,
}
impl Harness {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            data: dir.path().join("data"),
            state: dir.path().join("state"),
            config: dir.path().join("config"),
        };
        let t3 = Arc::new(FakeT3::default());
        let app = Daemon::open(
            paths.clone(),
            t3.clone(),
            Arc::new(FakeSpeech::default()),
            Arc::new(FakeBrowser::default()),
            Arc::new(NoopHotkeys),
        )
        .await
        .unwrap();
        app.seed_demo().await.unwrap();
        app.recover().await.unwrap();
        let token = std::fs::read_to_string(paths.state.join("token")).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = app.router();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            app,
            base,
            token,
            t3,
            server,
            _dir: dir,
            paths,
        }
    }
    async fn call(&self, method: &str, path: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                format!("{}/api/v1{path}", self.base),
            )
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        assert!(status.is_success(), "{path}: {status}: {text}");
        serde_json::from_str(&text).unwrap()
    }
    async fn state(&self) -> Value {
        self.call("GET", "/state", json!({})).await
    }
    async fn dispatch(&self) -> Value {
        let s = self.state().await;
        self.call(
            "POST",
            &format!(
                "/batches/{}/dispatch",
                s["status"]["activeBatchId"].as_str().unwrap()
            ),
            json!({}),
        )
        .await
    }
    async fn wait(&self, id: &str, status: &str) -> Value {
        for _ in 0..150 {
            let s = self.state().await;
            if let Some(t) = s["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == id && t["status"] == status)
            {
                return t.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("did not reach {status}: {}", self.state().await)
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.app.shutdown();
        self.server.abort();
    }
}
#[tokio::test]
async fn at10_at11_parallel_dispatch() {
    let h = Harness::new().await;
    let first = h.dispatch().await;
    assert_eq!(first["sealedBatch"]["state"], "sealed");
    let batch = first["activeBatch"]["id"].as_str().unwrap();
    h.call(
        "POST",
        "/annotations",
        json!({"batchId":batch,"type":"text","note":"Second task"}),
    )
    .await;
    let second = h.dispatch().await;
    let a = h
        .wait(first["task"]["id"].as_str().unwrap(), "running")
        .await;
    let b = h
        .wait(second["task"]["id"].as_str().unwrap(), "running")
        .await;
    assert_ne!(a["threadId"], b["threadId"]);
    assert_eq!(h.t3.thread_count(), 2);
    h.wait(a["id"].as_str().unwrap(), "completed").await;
}
#[tokio::test]
async fn at14_late_transcript_owned_by_old_batch() {
    let h = Harness::new().await;
    h.call("POST", "/actions/speech.toggle", json!({})).await;
    h.call("POST", "/actions/capture.dom", json!({})).await;
    let old = h.state().await["status"]["activeBatchId"].clone();
    let result = h.dispatch().await;
    assert_eq!(result["task"]["status"], "finalizing_transcript");
    let new = result["activeBatch"]["id"].clone();
    h.wait(result["task"]["id"].as_str().unwrap(), "completed")
        .await;
    let s = h.state().await;
    assert!(
        s["segments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|seg| seg["batchId"] == old)
    );
    assert!(
        s["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["batchId"] == new)
            .count()
            == 0
    );
    assert!(s["annotations"].as_array().unwrap().iter().any(|a|a["batchId"]==old && a["transcript"].as_str().is_some_and(|s|!s.is_empty())));
}
#[tokio::test]
async fn at15_failure_retry_no_duplicate() {
    let h = Harness::new().await;
    h.t3.set_mode(FakeMode::Failure);
    let d = h.dispatch().await;
    let id = d["task"]["id"].as_str().unwrap();
    h.wait(id, "failed").await;
    assert_eq!(h.t3.thread_count(), 0);
    h.t3.set_mode(FakeMode::Normal);
    h.call("POST", &format!("/tasks/{id}/retry"), json!({}))
        .await;
    h.wait(id, "completed").await;
    assert_eq!(h.t3.thread_count(), 1);
    assert_eq!(h.state().await["annotations"].as_array().unwrap().len(), 3);
}
#[tokio::test]
async fn at15_receipt_lost_adopts_marker() {
    let h = Harness::new().await;
    h.t3.set_mode(FakeMode::TimeoutAfter);
    let d = h.dispatch().await;
    let id = d["task"]["id"].as_str().unwrap();
    h.wait(id, "completed").await;
    assert_eq!(h.t3.thread_count(), 1);
}
#[tokio::test]
async fn at15_ambiguous_requires_resolution() {
    let h = Harness::new().await;
    h.t3.set_mode(FakeMode::TimeoutBefore);
    let d = h.dispatch().await;
    let id = d["task"]["id"].as_str().unwrap();
    h.wait(id, "unknown_remote_state").await;
    let response = reqwest::Client::new()
        .post(format!("{}/api/v1/tasks/{id}/retry", h.base))
        .bearer_auth(&h.token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    h.t3.set_mode(FakeMode::Normal);
    h.call(
        "POST",
        &format!("/tasks/{id}/resolve"),
        json!({"resolution":"not_sent"}),
    )
    .await;
    h.wait(id, "completed").await;
    assert_eq!(h.t3.thread_count(), 1);
}
#[tokio::test]
async fn at18_restart_persists_all_local_context() {
    let h = Harness::new().await;
    h.call("POST", "/actions/capture.dom", json!({})).await;
    let d = h.dispatch().await;
    let id = d["task"]["id"].as_str().unwrap();
    h.wait(id, "completed").await;
    let before = h.app.snapshot().await.unwrap();
    h.app.shutdown();
    let app = Daemon::fake(h.paths.clone()).await.unwrap();
    let after = app.snapshot().await.unwrap();
    assert_eq!(before, after);
    let a = after
        .annotations
        .iter()
        .find(|a| !a.assets.is_empty())
        .unwrap();
    let bytes = h.call("GET", "/state", json!({})).await;
    assert_eq!(bytes["annotations"].as_array().unwrap().len(), 4);
    let file = h
        .paths
        .batch_dir(after.batches.iter().find(|b| b.id == a.batch_id).unwrap())
        .unwrap()
        .join(&a.assets[0].path);
    assert!(file.exists());
    app.shutdown();
}
#[tokio::test]
async fn manifest_prompt_at19() {
    let h = Harness::new().await;
    h.call("POST", "/actions/capture.dom", json!({})).await;
    let d = h.dispatch().await;
    h.wait(d["task"]["id"].as_str().unwrap(), "completed").await;
    let b: Batch = serde_json::from_value(d["sealedBatch"].clone()).unwrap();
    let path = h.paths.batch_dir(&b).unwrap().join("batch.json");
    let manifest = std::fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(v["schemaVersion"], "1.0");
    assert!(!manifest.contains(&h.token));
    assert!(!manifest.contains("cookie"));
    assert!(!manifest.contains("authorization"));
    assert!(
        v["annotations"][3]["screenshot"]
            .as_str()
            .unwrap()
            .starts_with("assets/")
    );
    let prompt = build_prompt(path.to_str().unwrap()).unwrap();
    assert!(prompt.contains("je Annotation"));
    assert!(prompt.contains(path.to_str().unwrap()));
}
#[tokio::test]
async fn auth_and_one_time_exchange() {
    let h = Harness::new().await;
    let c = reqwest::Client::new();
    assert_eq!(
        c.get(format!("{}/api/v1/state", h.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let code = h.app.mint_code().await.unwrap();
    let response = c
        .post(format!("{}/api/v1/auth/exchange", h.base))
        .json(&json!({"code":code}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.json::<Value>().await.unwrap()["token"], h.token);
    assert_eq!(
        c.post(format!("{}/api/v1/auth/exchange", h.base))
            .json(&json!({"code":code}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}
#[tokio::test]
async fn websocket_snapshot_and_sequence() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let h = Harness::new().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!(
        "{}/api/v1/events",
        h.base.replace("http://", "ws://")
    ))
    .await
    .unwrap();
    socket
        .send(Message::Text(
            json!({"type":"auth","token":h.token}).to_string().into(),
        ))
        .await
        .unwrap();
    let first = socket.next().await.unwrap().unwrap();
    let v: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert_eq!(v["type"], "snapshot");
    assert_eq!(v["seq"], 1);
    h.call("POST", "/review-mode", json!({"on":true})).await;
    let mut seq = 1;
    loop {
        let next = tokio::time::timeout(Duration::from_secs(1), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let v: Value = serde_json::from_str(next.to_text().unwrap()).unwrap();
        seq += 1;
        assert_eq!(v["seq"], seq);
        if v["type"] == "status.changed" {
            assert_eq!(v["payload"]["reviewMode"], true);
            break;
        }
    }
    socket.close(None).await.unwrap();
}
#[tokio::test]
async fn binding_validation_is_atomic() {
    let h = Harness::new().await;
    let mut s = h.state().await["settings"].clone();
    s["bindings"][0]["scope"] = json!("global");
    let response = reqwest::Client::new()
        .put(format!("{}/api/v1/settings", h.base))
        .bearer_auth(&h.token)
        .json(&s)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    assert_eq!(h.state().await["settings"]["bindings"][0]["scope"], "page");
}
#[tokio::test]
async fn batch_followup_reuses_thread() {
    let h = Harness::new().await;
    let first = h.dispatch().await;
    let first_id = first["task"]["id"].as_str().unwrap();
    let first_task = h.wait(first_id, "completed").await;
    let batch = first["activeBatch"]["id"].as_str().unwrap();
    h.call(
        "POST",
        "/annotations",
        json!({"batchId":batch,"type":"text","note":"Follow-up change"}),
    )
    .await;
    let second = h
        .call(
            "POST",
            &format!("/batches/{batch}/dispatch"),
            json!({"followUpTaskId":first_id}),
        )
        .await;
    let second_task = h
        .wait(second["task"]["id"].as_str().unwrap(), "completed")
        .await;
    assert_eq!(first_task["threadId"], second_task["threadId"]);
    assert_eq!(h.t3.thread_count(), 1);
}
#[tokio::test]
async fn approvals_and_workspace_conflicts() {
    let h = Harness::new().await;
    h.t3.set_mode(FakeMode::Approval);
    let state = h.state().await;
    let profile = state["profiles"][0]["id"].as_str().unwrap();
    h.call(
        "PATCH",
        &format!("/profiles/{profile}"),
        json!({"workspaceStrategy":"shared_root"}),
    )
    .await;
    let first = h.dispatch().await;
    let a = h
        .wait(first["task"]["id"].as_str().unwrap(), "needs_approval")
        .await;
    h.call(
        "POST",
        "/annotations",
        json!({"batchId":first["activeBatch"]["id"],"type":"text","note":"parallel"}),
    )
    .await;
    let second = h.dispatch().await;
    let b = h
        .wait(second["task"]["id"].as_str().unwrap(), "needs_approval")
        .await;
    assert_eq!(a["pendingRequest"], "Bitte bestätigen.");
    assert_eq!(b["workspaceConflict"], true);
}
#[tokio::test]
async fn restart_during_running_tracks_same_run() {
    let h = Harness::new().await;
    let d = h.dispatch().await;
    let id = d["task"]["id"].as_str().unwrap();
    let before = h.wait(id, "running").await;
    h.app.shutdown();
    let app = Daemon::open(
        h.paths.clone(),
        h.t3.clone(),
        Arc::new(FakeSpeech::default()),
        Arc::new(FakeBrowser::default()),
        Arc::new(NoopHotkeys),
    )
    .await
    .unwrap();
    app.recover().await.unwrap();
    for _ in 0..100 {
        let s = app.snapshot().await.unwrap();
        let t = s.tasks.iter().find(|t| t.id == id).unwrap();
        if t.status == TaskStatus::Completed {
            assert_eq!(t.thread_id.as_deref(), before["threadId"].as_str());
            assert_eq!(h.t3.thread_count(), 1);
            app.shutdown();
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app.shutdown();
    panic!("recovered worker did not complete");
}
#[tokio::test]
async fn explicit_followup_is_durable_and_keeps_thread() {
    let h = Harness::new().await;
    let first = h.dispatch().await;
    let id = first["task"]["id"].as_str().unwrap();
    let done = h.wait(id, "completed").await;
    let annotation = h.state().await["annotations"][0]["id"].clone();
    let queued = h
        .call(
            "POST",
            &format!("/tasks/{id}/follow-up"),
            json!({"text":"Please recheck","annotationIds":[annotation]}),
        )
        .await;
    assert_eq!(queued["status"], "queued");
    let result = h.wait(id, "completed").await;
    assert_eq!(result["threadId"], done["threadId"]);
    assert_eq!(h.t3.thread_count(), 1);
    let events = h
        .call("GET", &format!("/tasks/{id}/events"), json!({}))
        .await;
    assert!(events.as_array().unwrap().iter().any(
        |e| e["kind"] == "follow_up" && e["text"].as_str().unwrap().contains("Please recheck")
    ));
}
#[tokio::test]
async fn extend_capture_keeps_annotation_and_original_assets() {
    let h = Harness::new().await;
    let mut settings = h.state().await["settings"].clone();
    settings["capture"]["mode"] = json!("extend_open_entry");
    h.call("PUT", "/settings", settings).await;
    h.call("POST", "/actions/capture.viewport", json!({})).await;
    h.call("POST", "/actions/capture.dom", json!({})).await;
    let state = h.state().await;
    let a = state["annotations"].as_array().unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a[2]["assets"].as_array().unwrap().len(), 2);
    assert_ne!(a[2]["assets"][0]["path"], a[2]["assets"][1]["path"]);
}
#[tokio::test]
async fn cli_status_action_open_never_print_token() {
    let h = Harness::new().await;
    let port = h.base.rsplit(':').next().unwrap();
    for args in [
        vec!["status"],
        vec!["action", "review.toggle"],
        vec!["open"],
    ] {
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_zuruf"))
            .args(&args)
            .env("ZURUF_DATA_DIR", &h.paths.data)
            .env("ZURUF_STATE_DIR", &h.paths.state)
            .env("ZURUF_CONFIG_DIR", &h.paths.config)
            .env("ZURUF_PORT", port)
            .env("ZURUF_FAKE_ADAPTERS", "1")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains(&h.token));
        if args == ["open"] {
            assert!(text.contains("/#code="));
        }
    }
}
#[test]
fn manifest_redacts_sensitive_metadata_and_rejects_traversal() {
    let snapshot: Snapshot = serde_json::from_str(include_str!("fixtures/Snapshot.json")).unwrap();
    let mut annotation = snapshot.annotations[0].clone();
    annotation.url =
        Some("https://name:password@localhost/page?token=TOP_SECRET&ok=yes#cookie".into());
    annotation
        .dom
        .as_mut()
        .unwrap()
        .attributes
        .insert("cookie".into(), "PRIVATE_COOKIE".into());
    annotation
        .dom
        .as_mut()
        .unwrap()
        .attributes
        .insert("authorization".into(), "PRIVATE_AUTH".into());
    annotation.assets[0].path = "assets/ann-001.png".into();
    let settings = SettingsRedaction {
        strip_query: false,
        patterns: vec![],
    };
    let result = build_manifest(
        &snapshot.batches[0],
        &snapshot.projects[0],
        &snapshot.profiles[0],
        &[annotation.clone()],
        &settings,
    )
    .unwrap()
    .to_string();
    for secret in [
        "TOP_SECRET",
        "PRIVATE_COOKIE",
        "PRIVATE_AUTH",
        "name:password",
    ] {
        assert!(!result.contains(secret));
    }
    assert!(result.contains("ok=yes"));
    annotation.assets[0].path = "../token".into();
    assert!(
        build_manifest(
            &snapshot.batches[0],
            &snapshot.projects[0],
            &snapshot.profiles[0],
            &[annotation],
            &settings
        )
        .is_err()
    );
}

#[tokio::test]
async fn panel_cors_allows_only_the_built_extension() {
    let h = Harness::new().await;
    let origin = zuruf::real::extension_origin();
    let client = reqwest::Client::new();
    let preflight = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{}/api/v1/settings", h.base),
        )
        .header("origin", &origin)
        .header("access-control-request-method", "PUT")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), 204);
    assert_eq!(preflight.headers()["access-control-allow-origin"], origin);
    let allowed = client
        .get(format!("{}/api/v1/state", h.base))
        .bearer_auth(&h.token)
        .header("origin", &origin)
        .send()
        .await
        .unwrap();
    assert!(allowed.status().is_success());
    assert_eq!(allowed.headers()["access-control-allow-origin"], origin);
    let denied = client
        .get(format!("{}/api/v1/state", h.base))
        .bearer_auth(&h.token)
        .header("origin", "chrome-extension://untrusted")
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    assert!(
        denied
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[tokio::test]
async fn dispatch_credentials_are_private_and_revoked_on_cancel() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::new().await;
    let sid = h.state().await["status"]["activeSessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    h.call("POST", &format!("/sessions/{sid}/start"), json!({}))
        .await;
    h.call("POST", "/actions/capture.viewport", json!({})).await;
    let dispatched = h.dispatch().await;
    let task = dispatched["task"]["id"].as_str().unwrap();
    let credential = h
        .paths
        .state
        .join("agents")
        .join(task)
        .join("credential.json");
    for _ in 0..100 {
        if credential.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::metadata(&credential).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let private: Value = serde_json::from_slice(&std::fs::read(&credential).unwrap()).unwrap();
    let secret = private["token"].as_str().unwrap();
    assert!(h.app.mcp_agent_scope(secret).is_some());
    assert!(
        !serde_json::to_string(&h.state().await)
            .unwrap()
            .contains(secret)
    );
    h.call("POST", &format!("/tasks/{task}/cancel"), json!({}))
        .await;
    assert!(!credential.exists());
    assert!(h.app.mcp_agent_scope(secret).is_none());
}
