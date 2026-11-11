//! Task-scoped MCP boundary. No dependency on daemon internals.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    future::Future,
    io::Read,
    pin::Pin,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use zuruf_core::*;
pub type ApiFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;
/// Implementations must recheck scope at mutation/asset access time.
pub trait ZurufApi: Send + Sync {
    fn snapshot(&self) -> ApiFuture<'_, Snapshot>;
    fn asset(&self, scope: Scope, asset: String) -> ApiFuture<'_, Vec<u8>>;
    fn update(
        &self,
        scope: Scope,
        annotation: Option<String>,
        value: Value,
    ) -> ApiFuture<'_, Value>;
    /// Must use the browser's authoritative arbiter (also used by human takeover).
    fn control(&self, scope: Scope, action: String, seconds: u64) -> ApiFuture<'_, Value>;
    /// Mark an element in the session's visible tab for the person at the browser, with a
    /// message. Needs no control of the browser: it only shows, it does not act.
    fn show(&self, scope: Scope, selector: String, message: String) -> ApiFuture<'_, Value>;
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub project_id: String,
    pub task_id: String,
    pub session_id: String,
}
struct Grant {
    scope: Scope,
    expires: Instant,
}
#[derive(Clone, Default)]
pub struct AgentAuth(Arc<RwLock<BTreeMap<String, Grant>>>);
/// Deliberately not Debug or Serialize: deliver only through a private runtime channel.
pub struct Handoff {
    pub url: String,
    pub origin: String,
    pub token: String,
}
impl AgentAuth {
    /// Called by dispatch after validating task/project/session relationships.
    /// Never put the returned token in a prompt, manifest, log or repository.
    pub fn issue(&self, scope: Scope, port: u16, ttl: Duration) -> std::io::Result<Handoff> {
        let mut random = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut grants = self
            .0
            .write()
            .map_err(|_| std::io::Error::other("auth lock"))?;
        grants.retain(|_, g| g.expires > Instant::now());
        grants.insert(
            token.clone(),
            Grant {
                scope,
                expires: Instant::now() + ttl.min(Duration::from_secs(86400)),
            },
        );
        Ok(Handoff {
            url: format!("http://127.0.0.1:{port}/mcp"),
            origin: format!("http://127.0.0.1:{port}"),
            token,
        })
    }
    pub fn revoke_task(&self, task: &str) {
        if let Ok(mut g) = self.0.write() {
            g.retain(|_, g| g.scope.task_id != task);
        }
    }
    pub fn scope(&self, token: &str) -> Option<Scope> {
        self.0
            .read()
            .ok()?
            .get(token)
            .filter(|g| g.expires > Instant::now())
            .map(|g| g.scope.clone())
    }
}
#[derive(Clone)]
struct Server {
    api: Arc<dyn ZurufApi>,
    auth: AgentAuth,
}
/// Mount this router at `/mcp`; GET intentionally returns 405 (no SSE stream).
pub fn router(api: Arc<dyn ZurufApi>, auth: AgentAuth) -> Router {
    Router::new()
        .route("/mcp", any(handle))
        .with_state(Server { api, auth })
}
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
fn error(id: Value, code: i32, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}
async fn handle(
    State(s): State<Server>,
    headers: HeaderMap,
    method: Method,
    body: axum::body::Bytes,
) -> Response {
    let host = headers
        .get("host")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    if !host.starts_with("127.0.0.1:")
        || headers.get("origin").and_then(|x| x.to_str().ok())
            != Some(format!("http://{host}").as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let token = headers
        .get("authorization")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Bearer "))
        .unwrap_or("");
    let Some(scope) = s.auth.scope(token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !headers
        .get("content-type")
        .and_then(|x| x.to_str().ok())
        .is_some_and(|x| x.starts_with("application/json"))
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let Ok(v) = serde_json::from_slice::<Value>(&body) else {
        return error(Value::Null, -32700, "Parse error");
    };
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    if v["jsonrpc"] != "2.0"
        || !v["method"].is_string()
        || (v.get("id").is_some() && !(id.is_string() || id.is_number()))
    {
        return error(id, -32600, "Invalid Request");
    }
    let method = v["method"].as_str().unwrap_or("");
    if method != "initialize"
        && headers
            .get("mcp-protocol-version")
            .is_some_and(|h| !VERSIONS.iter().any(|v| h == *v))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if v.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match method {
        "initialize" => {
            let requested = v["params"]["protocolVersion"].as_str().unwrap_or("");
            json!({"protocolVersion":if VERSIONS.contains(&requested){requested}else{VERSIONS[0]},"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"zuruf","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools":tools()}),
        "tools/call" => {
            let name = v["params"]["name"].as_str().unwrap_or("");
            let args = v["params"].get("arguments").cloned().unwrap_or(json!({}));
            if let Err(e) = validate(name, &args) {
                return error(id, -32602, &e);
            }
            match call(&s, scope, name, args).await {
                Ok(content) => json!({"content":content,"isError":false}),
                Err(_) => {
                    json!({"content":[{"type":"text","text":"Zugriff verweigert oder Aktion derzeit nicht möglich."}],"isError":true})
                }
            }
        }
        _ => return error(id, -32601, "Method not found"),
    };
    if s.auth.scope(token).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}
fn definitions() -> Vec<(&'static str, Value, Vec<&'static str>)> {
    let string = json!({"type":"string","minLength":1});
    let mut d = vec![];
    for n in [
        "list_projects",
        "list_sessions",
        "get_session_context",
        "list_reviews",
        "get_batch_manifest",
        "list_tasks",
        "release_browser_control",
        "get_cdp_endpoint",
    ] {
        d.push((n, json!({}), vec![]));
    }
    d.push(("list_annotations",json!({"batchId":string,"sessionId":string,"status":{"enum":["open","in_progress","done","wont_fix"]}}),vec![]));
    for n in ["get_annotation", "get_annotation_asset"] {
        let key = if n == "get_annotation" {
            "annotationId"
        } else {
            "assetId"
        };
        d.push((n, json!({key:string}), vec![key]));
    }
    d.push((
        "update_annotation_status",
        json!({"annotationId":string,"status":{"enum":["open","in_progress","done","wont_fix"]}}),
        vec!["annotationId", "status"],
    ));
    d.push((
        "report_progress",
        json!({"message":string,"needsReview":{"type":"boolean"}}),
        vec!["message"],
    ));
    d.push((
        "show_in_browser",
        json!({"selector":string,"message":string}),
        vec!["selector", "message"],
    ));
    for n in ["request_browser_control", "renew_browser_control"] {
        d.push((
            n,
            json!({"leaseSeconds":{"type":"integer","minimum":1,"maximum":300}}),
            vec![],
        ));
    }
    d
}
/// What a tool is for, where its name does not say it all.
fn description(name: &str) -> String {
    let purpose = match name {
        "show_in_browser" => {
            "Markiert im Browser des Nutzers das Element, auf das der CSS-Selektor passt, und zeigt daneben die Nachricht – zum Rückfragen oder Zeigen eines Ergebnisses. Braucht keine Browserkontrolle"
        }
        "report_progress" => {
            "Meldet, woran du gerade arbeitest; der Nutzer sieht die Nachricht in der Aufgabe und im Browser"
        }
        _ => name,
    };
    format!("{purpose}: nur für den zugewiesenen Task und dessen Session")
}
pub fn tools() -> Vec<Value> {
    definitions().into_iter().map(|(name,p,r)|json!({"name":name,"description":description(name),"inputSchema":{"type":"object","properties":p,"required":r,"additionalProperties":false}})).collect()
}
fn validate(name: &str, args: &Value) -> Result<(), String> {
    let (_, props, required) = definitions()
        .into_iter()
        .find(|(n, _, _)| *n == name)
        .ok_or("Unknown tool")?;
    let obj = args.as_object().ok_or("Object required")?;
    for k in required {
        if !obj.contains_key(k) {
            return Err(format!("Missing {k}"));
        }
    }
    for (k, v) in obj {
        let p = props.get(k).ok_or("Unknown argument")?;
        if p["type"] == "string" && v.as_str().is_none_or(|s| s.is_empty())
            || p["type"] == "boolean" && !v.is_boolean()
            || p["type"] == "integer" && v.as_u64().is_none_or(|n| !(1..=300).contains(&n))
            || p["enum"].as_array().is_some_and(|a| !a.contains(v))
        {
            return Err(format!("Invalid {k}"));
        }
    }
    Ok(())
}
async fn call(s: &Server, scope: Scope, name: &str, args: Value) -> Result<Vec<Value>, String> {
    let snap = s.api.snapshot().await?;
    let task = snap
        .tasks
        .iter()
        .find(|t| t.id == scope.task_id && t.project_id == scope.project_id)
        .ok_or("scope")?;
    let batch = snap
        .batches
        .iter()
        .find(|b| b.id == task.batch_id && b.project_id == scope.project_id)
        .ok_or("batch")?;
    let project = snap
        .projects
        .iter()
        .find(|p| p.id == scope.project_id)
        .ok_or("project")?;
    let session = snap
        .sessions
        .iter()
        .find(|s| s.id == scope.session_id && s.project_id == scope.project_id)
        .ok_or("session")?;
    let annotations = snap
        .annotations
        .iter()
        .filter(|a| {
            a.batch_id == batch.id
                && a.session_id
                    .as_deref()
                    .is_none_or(|id| id == scope.session_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut manifest = build_manifest(
        batch,
        project,
        &task.profile,
        &annotations,
        &snap.settings.redaction,
    )
    .map_err(|_| "manifest")?;
    manifest["transcripts"] = json!(
        snap.segments
            .iter()
            .filter(|s| s.batch_id == batch.id
                && s.annotation_id
                    .as_ref()
                    .is_none_or(|id| annotations.iter().any(|a| &a.id == id)))
            .collect::<Vec<_>>()
    );
    if let Some(entries) = manifest["annotations"].as_array_mut() {
        for entry in entries {
            if let Some(a) = annotations.iter().find(|a| a.id == entry["id"]) {
                entry["assetReferences"] = json!(
                    a.assets
                        .iter()
                        .filter(|a| a.kind != AssetKind::Audio)
                        .map(|a| json!({"id":a.id,"kind":a.kind,"width":a.width,"height":a.height}))
                        .collect::<Vec<_>>()
                );
                entry["status"] = json!(a.status);
            }
        }
    }
    let entries = manifest["annotations"].as_array().ok_or("annotations")?;
    let result = match name {
        "list_projects" => {
            json!([{"id":project.id,"name":project.name,"baseUrl":redact_url(&project.base_url,&snap.settings.redaction)}])
        }
        "list_reviews" => json!(
            snap.reviews
                .iter()
                .filter(|r| r.id == batch.review_id)
                .collect::<Vec<_>>()
        ),
        "list_sessions" | "get_session_context" => {
            json!({"id":session.id,"name":session.name,"mode":session.mode,"control":session.control,"tabs":session.tabs.iter().map(|t|json!({"id":t.id,"url":redact_url(&t.url,&snap.settings.redaction),"title":t.title})).collect::<Vec<_>>()})
        }
        "list_tasks" => {
            json!([{"id":task.id,"batchId":task.batch_id,"status":task.status,"title":task.title}])
        }
        "get_batch_manifest" => manifest.clone(),
        "list_annotations" => {
            if args.get("batchId").is_some_and(|v| v != &batch.id)
                || args.get("sessionId").is_some_and(|v| v != &session.id)
            {
                return Err("scope".into());
            }
            json!(
                entries
                    .iter()
                    .filter(|e| args
                        .get("status")
                        .is_none_or(|v| annotations.iter().any(|a| a.id == e["id"]
                            && serde_json::to_value(a.status).ok().as_ref() == Some(v))))
                    .collect::<Vec<_>>()
            )
        }
        "get_annotation" => entries
            .iter()
            .find(|a| a["id"] == args["annotationId"])
            .cloned()
            .ok_or("annotation")?,
        "get_annotation_asset" => {
            let id = args["assetId"].as_str().ok_or("asset")?;
            if !annotations.iter().any(|a| {
                a.assets
                    .iter()
                    .any(|x| x.id == id && x.kind != AssetKind::Audio)
            }) {
                return Err("asset".into());
            }
            let bytes = s.api.asset(scope, id.into()).await?;
            return Ok(vec![
                json!({"type":"image","mimeType":"image/png","data":STANDARD.encode(bytes)}),
            ]);
        }
        "update_annotation_status" => {
            let id = args["annotationId"].as_str().ok_or("annotation")?;
            if !annotations.iter().any(|a| a.id == id) {
                return Err("scope".into());
            }
            s.api
                .update(scope, Some(id.into()), args["status"].clone())
                .await?
        }
        "report_progress" => s.api.update(scope, None, args).await?,
        "show_in_browser" => {
            s.api
                .show(
                    scope,
                    args["selector"].as_str().ok_or("selector")?.into(),
                    args["message"].as_str().ok_or("message")?.into(),
                )
                .await?
        }
        "request_browser_control"
        | "renew_browser_control"
        | "release_browser_control"
        | "get_cdp_endpoint" => {
            s.api
                .control(
                    scope,
                    name.into(),
                    args["leaseSeconds"].as_u64().unwrap_or(60),
                )
                .await?
        }
        _ => return Err("tool".into()),
    };
    Ok(vec![
        json!({"type":"text","text":serde_json::to_string(&result).map_err(|_|"serialize")?}),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_expire_and_revoke_independently() {
        let auth = AgentAuth::default();
        let scope = |id: &str| Scope {
            project_id: "p".into(),
            task_id: id.into(),
            session_id: "s".into(),
        };
        let a = auth
            .issue(scope("a"), 47600, Duration::from_secs(60))
            .unwrap();
        let b = auth
            .issue(scope("b"), 47600, Duration::from_secs(60))
            .unwrap();
        assert!(auth.scope("ui-token").is_none());
        assert_eq!(auth.scope(&a.token).unwrap().task_id, "a");
        auth.revoke_task("a");
        assert!(auth.scope(&a.token).is_none());
        assert!(auth.scope(&b.token).is_some());
        let expired = auth.issue(scope("expired"), 47600, Duration::ZERO).unwrap();
        assert!(auth.scope(&expired.token).is_none());
    }
    #[test]
    fn input_validation_matches_wire_contract() {
        assert!(validate("get_annotation", &json!({})).is_err());
        assert!(validate("list_tasks", &json!({"taskId":"other"})).is_err());
        assert!(validate("request_browser_control", &json!({"leaseSeconds":301})).is_err());
        assert!(
            validate(
                "update_annotation_status",
                &json!({"annotationId":"a","status":"done"})
            )
            .is_ok()
        );
        assert!(
            validate(
                "update_annotation_status",
                &json!({"annotationId":"a","status":"resolved"})
            )
            .is_err()
        );
    }
}
