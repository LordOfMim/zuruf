//! Loopback T3 integration. Credentials are never included in diagnostics.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP transport failed")]
    Http(#[from] reqwest::Error),
    #[error("T3 rejected operation: {0}")]
    Protocol(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub model_options: bool,
    pub existing_worktree: bool,
    pub idempotent_launch: bool,
    pub interrupt: bool,
    pub attachments: bool,
    pub push: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Connection {
    pub state: String,
    pub version: Option<String>,
    pub transport: Option<String>,
    pub capabilities: Capabilities,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3ProjectRef {
    pub id: String,
    pub title: String,
    pub workspace_root: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub provider_instance_id: String,
    pub model: String,
    pub options: BTreeMap<String, Value>,
    pub runtime_mode: Option<String>,
    pub interaction_mode: Option<String>,
    pub workspace_strategy: WorkspaceStrategy,
    pub base_ref: Option<String>,
    pub worktree_path: Option<String>,
    pub slot: Option<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStrategy {
    NewWorktree,
    ExistingWorktree,
    SharedRoot,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Model {
    pub id: String,
    pub label: Option<String>,
    #[serde(default, deserialize_with = "null_vec")]
    pub options: Vec<ModelOption>,
}
fn null_vec<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub values: Vec<Value>,
    pub default: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Provider {
    pub instance_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub constraints: Vec<String>,
    pub models: Vec<T3Model>,
    pub omitted_models: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Catalog {
    pub fetched_at: String,
    pub providers: Vec<T3Provider>,
    pub runtime_modes: Vec<String>,
    pub interaction_modes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReceipt {
    pub thread_id: String,
    pub link: Option<String>,
    pub run_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    NeedsReview,
    NeedsApproval,
    Failed,
    Cancelled,
    UnknownRemoteState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStatus {
    pub remote_status: String,
    pub status: TaskStatus,
    pub last_message: Option<String>,
    pub pending_request: Option<String>,
    pub pending_kind: Option<String>,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    session_id: String,
    origin: String,
}

pub fn pkce(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
pub fn parse_rpc(body: &str) -> Result<Value> {
    let value: Value = if body.trim_start().starts_with('{') {
        serde_json::from_str(body)?
    } else {
        let normalized = body.replace("\r\n", "\n");
        let mut found = None;
        for event in normalized.split("\n\n") {
            let data = event
                .lines()
                .filter_map(|l| l.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                let v: Value = serde_json::from_str(&data)?;
                if v.get("id").is_some() {
                    found = Some(v);
                }
            }
        }
        found.ok_or_else(|| Error::Protocol("SSE contained no RPC response".into()))?
    };
    if value.get("error").is_some() {
        return Err(Error::Protocol("JSON-RPC error (details withheld)".into()));
    }
    Ok(value["result"].clone())
}
pub fn detect_capabilities(tools: &Value) -> Capabilities {
    let list = tools["tools"].as_array().cloned().unwrap_or_default();
    let launch = list.iter().find(|t| t["name"] == "t3_thread_launch");
    Capabilities {
        model_options: launch
            .and_then(|t| t.pointer("/inputSchema/properties/modelSelection/properties/options"))
            .is_some(),
        existing_worktree: launch
            .is_some_and(|t| t["inputSchema"].to_string().contains("existing_worktree")),
        // A request key in a schema is not a cross-session idempotency guarantee.
        idempotent_launch: false,
        interrupt: list.iter().any(|t| t["name"] == "t3_thread_interrupt"),
        attachments: list
            .iter()
            .any(|t| t["name"] == "t3_attachment_prepare_upload")
            && list
                .iter()
                .any(|t| t["name"] == "t3_thread_send_attachments"),
        push: false,
    }
}
pub fn map_status(remote: &str, pending_kind: Option<&str>) -> TaskStatus {
    if matches!(remote, "failed" | "cancelled" | "interrupted") {
        return match remote {
            "failed" => TaskStatus::Failed,
            _ => TaskStatus::Cancelled,
        };
    }
    if pending_kind == Some("approval") {
        return TaskStatus::NeedsApproval;
    }
    if pending_kind.is_some() {
        return TaskStatus::NeedsReview;
    }
    match remote {
        "preparing" | "queued" => TaskStatus::Queued,
        "starting" | "running" => TaskStatus::Running,
        "completed" => TaskStatus::Completed,
        "waiting" => TaskStatus::NeedsReview,
        "failed" => TaskStatus::Failed,
        "interrupted" | "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::UnknownRemoteState,
    }
}
fn string(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_owned)
}
fn unpack(v: Value) -> Result<Value> {
    if v["isError"] == true {
        return Err(Error::Protocol(
            "MCP tool rejected request (details withheld)".into(),
        ));
    }
    if let Some(s) = v.get("structuredContent") {
        return Ok(s.clone());
    }
    let text = v["content"]
        .as_array()
        .and_then(|a| a.iter().find_map(|c| c["text"].as_str()))
        .ok_or_else(|| Error::Protocol("Missing tool output".into()))?;
    Ok(serde_json::from_str(text)?)
}

pub struct T3Adapter {
    cli: PathBuf,
    credentials_path: PathBuf,
    http: reqwest::Client,
    credentials: Option<Credentials>,
    session: Option<String>,
    sequence: u64,
    pub connection: T3Connection,
    tools: Value,
}
impl T3Adapter {
    pub fn new(cli: PathBuf, credentials_path: PathBuf) -> Result<Self> {
        Ok(Self {
            cli,
            credentials_path,
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(65))
                .build()?,
            credentials: None,
            session: None,
            sequence: 0,
            tools: Value::Null,
            connection: T3Connection {
                state: "disconnected".into(),
                version: None,
                transport: None,
                capabilities: Capabilities::default(),
                error: None,
            },
        })
    }
    async fn cli(&self, args: &[String], stdin: Option<&str>) -> Result<Value> {
        let mut child = Command::new(&self.cli)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        if let Some(mut input) = child.stdin.take()
            && let Some(text) = stdin
        {
            input.write_all(text.as_bytes()).await?;
        }
        let out = child.wait_with_output().await?;
        if !out.status.success() {
            return Err(Error::Protocol(format!("CLI exit {:?}", out.status.code())));
        }
        Ok(serde_json::from_slice(&out.stdout)?)
    }
    async fn pair(&mut self, origin: &str) -> Result<()> {
        let origin = validate_origin(origin)?;
        let meta: Value = self
            .http
            .get(format!("{origin}/.well-known/oauth-authorization-server"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let token_endpoint = meta["token_endpoint"]
            .as_str()
            .ok_or_else(|| Error::Protocol("Missing token endpoint".into()))?;
        if !token_endpoint.starts_with(&format!("{origin}/")) {
            return Err(Error::Protocol("Nonlocal OAuth endpoint".into()));
        }
        let label = format!("Zuruf adapters {}", uuid::Uuid::new_v4());
        let redirect = "http://127.0.0.1:39871/callback";
        let client: Value = self.http.post(format!("{origin}/oauth/mcp/register")).json(&json!({"client_name":label,"redirect_uris":[redirect],"token_endpoint_auth_method":"none"})).send().await?.error_for_status()?.json().await?;
        let pairing = self
            .cli(
                &[
                    "auth",
                    "pairing",
                    "create",
                    "--scope",
                    "orchestration:read",
                    "--scope",
                    "orchestration:operate",
                    "--ttl",
                    "5m",
                    "--label",
                    "zuruf-adapters",
                    "--json",
                ]
                .map(str::to_owned),
                None,
            )
            .await?;
        let verifier = format!(
            "{}{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let state = uuid::Uuid::new_v4().to_string();
        let authorization = json!({"response_type":"code","client_id":client["client_id"],"redirect_uri":redirect,"code_challenge":pkce(&verifier),"code_challenge_method":"S256","resource":format!("{origin}/mcp"),"state":state});
        let decision: Value = self.http.post(format!("{origin}/oauth/mcp/decision")).json(&json!({"authorization":authorization,"decision":{"_tag":"pairing-code","access":"approval-required","code":pairing["credential"]}})).send().await?.error_for_status()?.json().await?;
        let url = reqwest::Url::parse(decision["redirectTo"].as_str().unwrap_or(""))
            .map_err(|_| Error::Protocol("Bad redirect".into()))?;
        let params: BTreeMap<_, _> = url
            .query_pairs()
            .map(|(a, b)| (a.into_owned(), b.into_owned()))
            .collect();
        if params.get("state") != Some(&state) {
            return Err(Error::Protocol("OAuth state mismatch".into()));
        }
        let token: Value = self.http.post(token_endpoint).form(&json!({"grant_type":"authorization_code","client_id":client["client_id"],"redirect_uri":redirect,"code":params.get("code"),"code_verifier":verifier,"resource":format!("{origin}/mcp")})).send().await?.error_for_status()?.json().await?;
        let access_token = string(&token, "access_token")
            .ok_or_else(|| Error::Protocol("No access token".into()))?;
        let sessions = self
            .cli(
                &["auth", "session", "list", "--json"].map(str::to_owned),
                None,
            )
            .await?;
        let session_id = sessions
            .as_array()
            .and_then(|a| {
                a.iter()
                    .filter(|s| {
                        s.pointer("/client/label").and_then(Value::as_str) == Some(label.as_str())
                    })
                    .max_by_key(|s| s["createdAt"].as_str())
            })
            .and_then(|s| string(s, "sessionId"))
            .ok_or_else(|| Error::Protocol("Cannot identify revocable session".into()))?;
        self.credentials = Some(Credentials {
            access_token,
            session_id,
            origin,
        });
        if let Err(error) = self.save().await {
            if let Some(credentials) = self.credentials.take() {
                let _ = Command::new(&self.cli)
                    .args(["auth", "session", "revoke", &credentials.session_id])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await;
            }
            return Err(error);
        }
        Ok(())
    }
    async fn save(&self) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let bytes = serde_json::to_vec(&self.credentials)?;
        let path = self.credentials_path.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
            let result = (|| {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temporary)?;
                f.write_all(&bytes)?;
                f.sync_all()?;
                std::fs::rename(&temporary, &path)?;
                if let Some(parent) = path.parent() {
                    std::fs::File::open(parent)?.sync_all()?;
                }
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(temporary);
            }
            result
        })
        .await
        .map_err(|_| Error::Protocol("Credential write task failed".into()))??;
        Ok(())
    }
    pub async fn connect(&mut self, origin: &str) -> Result<()> {
        let result = self.connect_inner(origin).await;
        if let Err(error) = &result {
            self.connection.state = "error".into();
            self.connection.error = Some(error.to_string());
        }
        result
    }
    async fn connect_inner(&mut self, origin: &str) -> Result<()> {
        self.connection.state = "connecting".into();
        self.connection.version = None;
        self.connection.error = None;
        let origin = validate_origin(origin)?;
        if let Ok(metadata) = tokio::fs::symlink_metadata(&self.credentials_path).await
            && (!metadata.is_file() || metadata.permissions().mode() & 0o077 != 0)
        {
            return Err(Error::Protocol(
                "Credential file must be a private regular file (0600)".into(),
            ));
        }
        self.credentials = None;
        if let Ok(bytes) = tokio::fs::read(&self.credentials_path).await {
            self.credentials = serde_json::from_slice::<Option<Credentials>>(&bytes)?;
        }
        if self.credentials.as_ref().is_none_or(|c| c.origin != origin) {
            self.pair(&origin).await?;
        }
        self.session = None;
        let init = json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Zuruf","version":"0.1"}});
        // Transport failures never rotate OAuth identity: replay reconciliation depends on it.
        let value = self.rpc("initialize", init).await?;
        self.connection.version = string(&value["serverInfo"], "version");
        self.tools = self.rpc("tools/list", json!({})).await?;
        self.connection.capabilities = detect_capabilities(&self.tools);
        self.connection.state = "connected".into();
        self.connection.transport = Some("mcp".into());
        tracing::info!("T3 MCP connected");
        Ok(())
    }
    pub async fn connect_cli(&mut self) -> Result<()> {
        let output = Command::new(&self.cli).arg("--version").output().await?;
        if !output.status.success() {
            return Err(Error::Protocol("CLI unavailable".into()));
        }
        self.connection = T3Connection {
            state: "connected".into(),
            transport: Some("cli".into()),
            version: Some(String::from_utf8_lossy(&output.stdout).trim().into()),
            capabilities: Capabilities::default(),
            error: None,
        };
        Ok(())
    }
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(c) = &self.credentials {
            let out = Command::new(&self.cli)
                .args(["auth", "session", "revoke", &c.session_id])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await?;
            if !out.status.success() {
                return Err(Error::Protocol("Session revocation failed".into()));
            }
        }
        self.credentials = None;
        self.session = None;
        match tokio::fs::remove_file(&self.credentials_path).await {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        self.connection.state = "disconnected".into();
        Ok(())
    }
    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let c = self
            .credentials
            .as_ref()
            .ok_or_else(|| Error::Protocol("Disconnected".into()))?;
        self.sequence += 1;
        let mut request = self
            .http
            .post(format!("{}/mcp", c.origin))
            .bearer_auth(&c.access_token)
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18");
        if let Some(s) = &self.session {
            request = request.header("mcp-session-id", s);
        }
        let response = request
            .json(&json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params}))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Error::Protocol(format!("MCP HTTP {}", response.status())));
        }
        if let Some(s) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|s| s.to_str().ok())
        {
            self.session = Some(s.into());
        }
        parse_rpc(&response.text().await?)
    }
    pub async fn call(&mut self, name: &str, arguments: Value) -> Result<Value> {
        if self.connection.transport.as_deref() == Some("cli") {
            return Err(Error::Unsupported(format!("MCP-only tool: {name}")));
        }
        let response = self
            .rpc("tools/call", json!({"name":name,"arguments":arguments}))
            .await;
        match response {
            Ok(value) => unpack(value),
            Err(Error::Protocol(reason)) if reason.starts_with("MCP HTTP 401") => {
                Err(Error::Protocol(
                    "OAuth authorization expired; pair explicitly, then reconcile remote state before retrying".into(),
                ))
            }
            Err(error) => {
                self.connection.state = "error".into();
                self.connection.error = Some(error.to_string());
                Err(error)
            }
        }
    }
    /// Advertised external upload transport, verified against local T3 v0.1.7.
    pub async fn upload_image(
        &mut self,
        name: &str,
        mime_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Value> {
        if !self.connection.capabilities.attachments {
            return Err(Error::Unsupported("image upload".into()));
        }
        if bytes.is_empty()
            || bytes.len() > 10 * 1024 * 1024
            || !["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&mime_type)
        {
            return Err(Error::Unsupported("image format/size".into()));
        }
        let size = bytes.len();
        let prepared=self.call("t3_attachment_prepare_upload",json!({"upload":{"type":"image","name":name,"mimeType":mime_type,"sizeBytes":size}})).await?;
        let credentials = self
            .credentials
            .as_ref()
            .ok_or_else(|| Error::Protocol("Disconnected".into()))?;
        let base = reqwest::Url::parse(&credentials.origin)
            .map_err(|_| Error::Protocol("Invalid origin".into()))?;
        let url = base
            .join(
                prepared["relativeUrl"]
                    .as_str()
                    .ok_or_else(|| Error::Protocol("Missing upload URL".into()))?,
            )
            .map_err(|_| Error::Protocol("Invalid upload URL".into()))?;
        if url.origin() != base.origin() {
            return Err(Error::Protocol("Nonlocal upload URL".into()));
        }
        let response = self
            .http
            .post(url)
            .bearer_auth(&credentials.access_token)
            .header("Content-Type", mime_type)
            .body(bytes)
            .send()
            .await;
        let failure = match response {
            Ok(response) if response.status().is_success() => {
                return Ok(
                    json!({"type":"image","id":prepared["attachmentId"],"name":name,"mimeType":mime_type,"sizeBytes":size}),
                );
            }
            Ok(response) => Error::Protocol(format!("Upload HTTP {}", response.status())),
            Err(error) => Error::Http(error),
        };
        // No thread launch has occurred, so even an uncertain upload is pending only.
        if let Some(id) = prepared["attachmentId"].as_str() {
            let _ = self.discard_upload(id).await;
        }
        Err(failure)
    }
    pub async fn discard_upload(&mut self, attachment_id: &str) -> Result<()> {
        self.call(
            "t3_attachment_discard",
            json!({"attachmentId":attachment_id}),
        )
        .await?;
        Ok(())
    }
    pub async fn send_attachments(
        &mut self,
        thread: &str,
        text: &str,
        attachments: Vec<Value>,
    ) -> Result<ThreadReceipt> {
        let value = self
            .call(
                "t3_thread_send_attachments",
                json!({"threadId":thread,"message":text,"attachments":attachments}),
            )
            .await?;
        Ok(serde_json::from_value(value)?)
    }
    pub async fn list_projects(&mut self) -> Result<Vec<T3ProjectRef>> {
        let mut result = Vec::new();
        let mut cursor = 0;
        loop {
            let v = self
                .call("t3_project_list", json!({"cursor":cursor,"limit":100}))
                .await?;
            result.extend(serde_json::from_value::<Vec<T3ProjectRef>>(
                v["projects"].clone(),
            )?);
            match v["nextCursor"].as_u64() {
                Some(n) if n > cursor => cursor = n,
                _ => break,
            }
        }
        Ok(result)
    }
    pub async fn catalog(&mut self, search: Option<&str>) -> Result<T3Catalog> {
        let v = self
            .call(
                "orchestrator_capabilities",
                search.map_or(json!({}), |s| json!({"search":s})),
            )
            .await?;
        let providers = v["providers"]
            .as_array()
            .ok_or_else(|| Error::Protocol("Missing providers".into()))?
            .iter()
            .map(|p| {
                Ok(T3Provider {
                    instance_id: string(p, "providerInstanceId").unwrap_or_default(),
                    display_name: string(p, "displayName").unwrap_or_default(),
                    enabled: p["canRunChildTask"].as_bool().unwrap_or(false),
                    constraints: serde_json::from_value(p["constraints"].clone())?,
                    models: serde_json::from_value(p["models"].clone())?,
                    omitted_models: p["omittedModels"].as_u64().unwrap_or(0) as usize,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let schema = self.tools["tools"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["name"] == "t3_thread_launch"))
            .cloned()
            .unwrap_or_default();
        Ok(T3Catalog {
            fetched_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| Error::Protocol("Timestamp".into()))?,
            providers,
            runtime_modes: serde_json::from_value(
                schema["inputSchema"]["properties"]["runtimeMode"]["enum"].clone(),
            )?,
            interaction_modes: serde_json::from_value(
                schema["inputSchema"]["properties"]["interactionMode"]["enum"].clone(),
            )?,
        })
    }
    pub async fn create_thread(
        &mut self,
        project: &str,
        title: &str,
        prompt: &str,
        profile: &AgentProfile,
        request_id: &str,
    ) -> Result<ThreadReceipt> {
        self.create_thread_with_attachments(project, title, prompt, profile, request_id, Vec::new())
            .await
    }
    pub async fn create_thread_with_attachments(
        &mut self,
        project: &str,
        title: &str,
        prompt: &str,
        profile: &AgentProfile,
        request_id: &str,
        attachments: Vec<Value>,
    ) -> Result<ThreadReceipt> {
        if attachments.len() > 8 {
            return Err(Error::Unsupported("At most eight images per launch".into()));
        }
        let title = format!("{title} [{request_id}]");
        let v = if self.connection.transport.as_deref() == Some("cli") {
            if !attachments.is_empty()
                || !profile.options.is_empty()
                || profile.runtime_mode.is_some()
                || profile.interaction_mode.is_some()
                || matches!(
                    profile.workspace_strategy,
                    WorkspaceStrategy::ExistingWorktree
                )
            {
                return Err(Error::Unsupported(
                    "CLI cannot honor profile options/modes/existing worktree".into(),
                ));
            }
            let mut args = vec![
                "thread".into(),
                "start".into(),
                project.into(),
                "-".into(),
                "--title".into(),
                title,
                "--provider".into(),
                profile.provider_instance_id.clone(),
                "--model".into(),
                profile.model.clone(),
                "--json".into(),
            ];
            if matches!(profile.workspace_strategy, WorkspaceStrategy::NewWorktree) {
                args.extend([
                    "--worktree".into(),
                    profile.base_ref.clone().unwrap_or("HEAD".into()),
                    "--branch".into(),
                    format!("zuruf-{request_id}"),
                ]);
            }
            self.cli(&args, Some(prompt)).await?
        } else {
            let projects = self.list_projects().await?;
            let project_id = projects
                .iter()
                .find(|p| p.id == project || p.workspace_root == project)
                .map(|p| p.id.clone())
                .ok_or_else(|| Error::Protocol("Project not found".into()))?;
            let workspace = match profile.workspace_strategy {
                WorkspaceStrategy::SharedRoot => json!({"type":"root"}),
                WorkspaceStrategy::ExistingWorktree => {
                    json!({"type":"existing_worktree","worktreePath":profile.worktree_path.as_ref().ok_or_else(||Error::Protocol("Missing worktree path".into()))?})
                }
                WorkspaceStrategy::NewWorktree => {
                    json!({"type":"worktree","baseRef":profile.base_ref.as_deref().unwrap_or("HEAD"),"branch":format!("zuruf-{request_id}"),"startFromOrigin":false})
                }
            };
            let mut args = json!({"projectId":project_id,"title":title,"message":prompt,"modelSelection":{"instanceId":profile.provider_instance_id,"model":profile.model,"options":profile.options},"workspaceStrategy":workspace,"clientRequestId":request_id});
            if !attachments.is_empty() {
                args["attachments"] = json!(attachments);
            }
            if let Some(mode) = &profile.runtime_mode {
                args["runtimeMode"] = json!(mode);
            }
            if let Some(mode) = &profile.interaction_mode {
                args["interactionMode"] = json!(mode);
            }
            self.call("t3_thread_launch", args).await?
        };
        let mut receipt: ThreadReceipt = serde_json::from_value(v)?;
        if self.connection.transport.as_deref() == Some("mcp") {
            let read = self
                .call(
                    "t3_thread_read",
                    json!({"threadId":receipt.thread_id,"limit":1}),
                )
                .await?;
            receipt.branch = string(&read["thread"], "branch");
            receipt.worktree_path = string(&read["thread"], "worktreePath");
        }
        Ok(receipt)
    }
    pub async fn send_message(
        &mut self,
        thread: &str,
        text: &str,
        request_id: &str,
    ) -> Result<ThreadReceipt> {
        let v = if self.connection.transport.as_deref() == Some("cli") {
            self.cli(
                &["thread", "send", thread, "-", "--json"].map(str::to_owned),
                Some(text),
            )
            .await?
        } else {
            self.call("t3_thread_send",json!({"threadId":thread,"message":text,"mode":"queue","clientRequestId":request_id})).await?
        };
        Ok(serde_json::from_value(v)?)
    }
    pub async fn thread_status(&mut self, thread: &str, run: &str) -> Result<ThreadStatus> {
        let v = if self.connection.transport.as_deref() == Some("cli") {
            self.cli(
                &["thread", "show", thread, "--messages", "100", "--json"].map(str::to_owned),
                None,
            )
            .await?
        } else {
            let mut all = json!([]);
            let mut position = None;
            loop {
                let mut args = json!({"threadId":thread,"view":"activity","limit":100,"runLimit":50,"maxCharsPerItem":50000});
                if let Some(p) = position {
                    args["afterPosition"] = json!(p);
                }
                let mut page = self.call("t3_thread_read", args).await?;
                if let (Some(a), Some(b)) = (all.as_array_mut(), page["items"].as_array()) {
                    a.extend(b.iter().cloned());
                }
                let next = page["nextPosition"].as_u64();
                if page["hasMore"] != true || next == position {
                    page["items"] = all;
                    break page;
                }
                position = next;
            }
        };
        Ok(parse_status(&v, run))
    }
    pub async fn find_threads_by_marker(
        &mut self,
        project: &str,
        marker: &str,
    ) -> Result<Vec<Value>> {
        if self.connection.transport.as_deref() == Some("mcp") {
            let projects = self.list_projects().await?;
            let id = projects
                .iter()
                .find(|p| p.id == project || p.workspace_root == project)
                .map(|p| p.id.clone())
                .ok_or_else(|| Error::Protocol("Project not found".into()))?;
            let mut threads = Vec::new();
            let mut cursor = 0;
            loop {
                let page = self
                    .call(
                        "t3_thread_list",
                        json!({"projectId":id,"titleContains":marker,"cursor":cursor,"limit":100}),
                    )
                    .await?;
                if let Some(items) = page["threads"].as_array() {
                    threads.extend(items.iter().cloned());
                } else {
                    return Err(Error::Protocol("Missing thread list".into()));
                }
                match page["nextCursor"].as_u64() {
                    Some(next) if next > cursor => cursor = next,
                    _ => break,
                }
            }
            return Ok(threads);
        }
        let v = self
            .cli(
                &["thread", "list", "--project", project, "--json"].map(str::to_owned),
                None,
            )
            .await?;
        let a = v
            .as_array()
            .or_else(|| v["threads"].as_array())
            .ok_or_else(|| Error::Protocol("Thread list shape".into()))?;
        Ok(a.iter()
            .filter(|t| t["title"].as_str().is_some_and(|s| s.contains(marker)))
            .cloned()
            .collect())
    }
    pub async fn interrupt(&mut self, thread: &str) -> Result<()> {
        if !self.connection.capabilities.interrupt {
            return Err(Error::Unsupported("interrupt".into()));
        }
        self.call("t3_thread_interrupt", json!({"threadId":thread}))
            .await?;
        Ok(())
    }
    /// One persistent HTTP client/session, adaptive polling, bounded event backpressure.
    pub async fn watch(
        &mut self,
        threads: Vec<(String, String)>,
        interval: Duration,
        events: tokio::sync::mpsc::Sender<(String, Result<ThreadStatus>)>,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) {
        let interval = interval.max(Duration::from_millis(100));
        let mut tracked: Vec<_> = threads
            .into_iter()
            .map(|(t, r)| (t, r, tokio::time::Instant::now(), 0u32, None))
            .collect();
        loop {
            if *stop.borrow() || events.is_closed() {
                break;
            }
            for (thread, run, due, idle, last) in &mut tracked {
                if tokio::time::Instant::now() < *due {
                    continue;
                }
                let result = self.thread_status(thread, run).await;
                let signature = result
                    .as_ref()
                    .ok()
                    .and_then(|s| serde_json::to_string(s).ok());
                *idle = if signature == *last {
                    (*idle + 1).min(5)
                } else {
                    0
                };
                *last = signature;
                *due = tokio::time::Instant::now() + interval * (1 << *idle);
                tokio::select! { _=stop.changed()=>return, sent=events.send((thread.clone(),result))=>{if sent.is_err(){return;}} }
            }
            tokio::select! { _=stop.changed()=>break, _=tokio::time::sleep(interval)=>() }
        }
    }
}
use std::os::unix::fs::PermissionsExt;
pub fn validate_origin(origin: &str) -> Result<String> {
    let url = reqwest::Url::parse(origin).map_err(|_| Error::Protocol("Invalid origin".into()))?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.path() != "/"
        || url.query().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Protocol(
            "Origin must be http://127.0.0.1:<port>".into(),
        ));
    }
    Ok(origin.trim_end_matches('/').into())
}
pub fn parse_status(v: &Value, run: &str) -> ThreadStatus {
    let runs = v["recentRuns"].as_array();
    let matched = runs.and_then(|a| a.iter().find(|r| r["runId"] == run));
    let remote = matched
        .and_then(|r| r["status"].as_str())
        .or_else(|| {
            if v["thread"]["latestRunId"] == run {
                v["thread"]["status"].as_str()
            } else {
                None
            }
        })
        .unwrap_or("unknown");
    let items = v["items"].as_array().or_else(|| v["messages"].as_array());
    let last = items
        .and_then(|a| {
            a.iter().rev().find(|m| {
                m["runId"] == run && (m["type"] == "assistant_message" || m["role"] == "assistant")
            })
        })
        .and_then(|m| string(m, "text"));
    let pending = items.and_then(|a| {
        a.iter().rev().find(|m| {
            m["runId"] == run
                && (m["status"] == "pending" || m["status"] == "waiting")
                && m["type"] != "assistant_message"
        })
    });
    let kind = pending.and_then(|p| string(p, "type"));
    let mapped_kind = kind.as_deref().map(|k| {
        if k.contains("approval") {
            "approval"
        } else {
            "question"
        }
    });
    ThreadStatus {
        remote_status: remote.into(),
        status: map_status(remote, mapped_kind),
        last_message: last,
        pending_request: pending.and_then(|p| string(p, "text").or_else(|| string(p, "title"))),
        pending_kind: kind,
        error: items
            .and_then(|a| {
                a.iter()
                    .rev()
                    .find(|m| m["runId"] == run && m["type"] == "error")
            })
            .and_then(|m| string(m, "text")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_rfc7636() {
        assert_eq!(
            pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn sse() {
        assert_eq!(
            parse_rpc(
                "event: message\r\ndata: {\"id\":1,\"result\":\r\ndata: {\"ok\":true}}\r\n\r\n"
            )
            .unwrap(),
            json!({"ok":true})
        );
    }
    #[test]
    fn fixtures() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/discovery.json")).unwrap();
        let tools = v
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v.get("tools").is_some())
            .unwrap();
        assert!(detect_capabilities(tools).model_options);
        assert!(!detect_capabilities(tools).idempotent_launch);
        let read = v
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v.get("read").is_some())
            .unwrap();
        let read = unpack(read["read"].clone()).unwrap();
        let run = read["thread"]["latestRunId"].as_str().unwrap();
        assert_eq!(parse_status(&read, run).last_message.as_deref(), Some("OK"));
        assert_eq!(
            parse_status(&read, "wrong").status,
            TaskStatus::UnknownRemoteState
        );
    }
    #[test]
    fn statuses() {
        assert_eq!(map_status("waiting", None), TaskStatus::NeedsReview);
        assert_eq!(
            map_status("waiting", Some("approval")),
            TaskStatus::NeedsApproval
        );
        assert_eq!(map_status("interrupted", None), TaskStatus::Cancelled);
        assert_eq!(
            map_status("rolled_back", None),
            TaskStatus::UnknownRemoteState
        );
    }
    #[test]
    fn recorded_states() {
        let states: Value =
            serde_json::from_str(include_str!("../tests/fixtures/states.json")).unwrap();
        for (key, status) in [
            ("questionRead", TaskStatus::NeedsReview),
            ("approvalRead", TaskStatus::NeedsApproval),
            ("failureRead", TaskStatus::Failed),
            ("interruptedRead", TaskStatus::Cancelled),
        ] {
            let v = &states[key];
            let run = v["thread"]["latestRunId"].as_str().unwrap();
            assert_eq!(parse_status(v, run).status, status);
        }
        let cli = json!({"thread":{"latestRunId":"new","status":"running"},"messages":[{"role":"assistant","runId":"old","text":"stale"}]});
        assert_eq!(parse_status(&cli, "new").last_message, None);
        assert_eq!(
            map_status("interrupted", Some("approval")),
            TaskStatus::Cancelled
        );
    }
    #[test]
    fn loopback() {
        assert!(validate_origin("http://example.com").is_err());
    }
    #[tokio::test]
    #[ignore = "creates one zuruf-test thread and sends one trivial follow-up"]
    async fn live_mutations() {
        let path = std::env::temp_dir().join(format!("zuruf-{}.json", uuid::Uuid::new_v4()));
        let mut adapter = T3Adapter::new("t3".into(), path).unwrap();
        adapter
            .connect(&std::env::var("T3_URL").unwrap_or("http://127.0.0.1:3773".into()))
            .await
            .unwrap();
        let result = async {
            let id = uuid::Uuid::new_v4().to_string();
            let profile = AgentProfile {
                id: id.clone(),
                name: "test".into(),
                provider_instance_id: "codex".into(),
                model: "gpt-6-luna".into(),
                options: BTreeMap::from([("reasoningEffort".into(), json!("low"))]),
                runtime_mode: Some("approval-required".into()),
                interaction_mode: Some("default".into()),
                workspace_strategy: WorkspaceStrategy::SharedRoot,
                base_ref: None,
                worktree_path: None,
                slot: None,
            };
            let receipt = adapter
                .create_thread(
                    "/home/mika/Projects/Humaify/zuruf/main",
                    "zuruf-test-rust",
                    "Reply exactly OK. Do not use tools or change files.",
                    &profile,
                    &id,
                )
                .await?;
            println!("LIVE_RECEIPT {}", serde_json::to_string(&receipt)?);
            let run = receipt
                .run_id
                .as_deref()
                .ok_or_else(|| Error::Protocol("Missing run".into()))?;
            adapter
                .call(
                    "t3_thread_wait",
                    json!({"threadId":receipt.thread_id,"runId":run,"timeoutMs":15000}),
                )
                .await?;
            let status = adapter.thread_status(&receipt.thread_id, run).await?;
            if status.status != TaskStatus::Completed {
                return Err(Error::Protocol(format!(
                    "Unexpected live status {:?}",
                    status.status
                )));
            }
            let follow = adapter
                .send_message(
                    &receipt.thread_id,
                    "Reply exactly OK. Do not use tools or change files.",
                    &uuid::Uuid::new_v4().to_string(),
                )
                .await?;
            let run = follow
                .run_id
                .as_deref()
                .ok_or_else(|| Error::Protocol("Missing follow-up run".into()))?;
            adapter
                .call(
                    "t3_thread_wait",
                    json!({"threadId":receipt.thread_id,"runId":run,"timeoutMs":15000}),
                )
                .await?;
            let status = adapter.thread_status(&receipt.thread_id, run).await?;
            if status.last_message.as_deref() != Some("OK") {
                return Err(Error::Protocol("Missing target-run OK".into()));
            }
            Ok::<_, Error>(())
        }
        .await;
        let disconnect = adapter.disconnect().await;
        result.unwrap();
        disconnect.unwrap();
    }
    #[tokio::test]
    #[ignore = "requires local T3 and creates/revokes an OAuth session"]
    async fn live_connect() {
        let path = std::env::temp_dir().join(format!("zuruf-{}.json", uuid::Uuid::new_v4()));
        let mut a = T3Adapter::new("t3".into(), path).unwrap();
        a.connect(&std::env::var("T3_URL").unwrap_or("http://127.0.0.1:3773".into()))
            .await
            .unwrap();
        assert!(!a.list_projects().await.unwrap().is_empty());
        assert!(!a.catalog(None).await.unwrap().providers.is_empty());
        assert!(
            !a.find_threads_by_marker(
                "/home/mika/Projects/Humaify/zuruf/main",
                "zuruf-test-root-1794330125378"
            )
            .await
            .unwrap()
            .is_empty()
        );
        a.disconnect().await.unwrap();
    }
}
