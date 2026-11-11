//! Mount after HTTP Upgrade at /cdp/<sessionId>. Authenticate before upgrading.
use crate::{
    Error, Result,
    cdp::{Cdp, Command},
    control::Arbitration,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;

pub fn authenticate(authorization: Option<&str>, token: &str) -> Result<()> {
    if token.is_empty() || authorization != Some(format!("Bearer {token}").as_str()) {
        return Err(Error::Denied("agent bearer token"));
    }
    Ok(())
}
pub fn page_target(info: &Value) -> bool {
    info["type"] == "page"
        && info["url"].as_str().is_some_and(|url| {
            // A tab that was just opened has no address yet; what it loads is checked again
            // before every command.
            url.is_empty()
                || url.starts_with("http://")
                || url.starts_with("https://")
                || url == "about:blank"
        })
}
/// A page an agent may see: a web page that is not one of the app's own, such as its panel.
fn agent_page(info: &Value, private: &HashSet<String>) -> bool {
    page_target(info)
        && !info["targetId"]
            .as_str()
            .is_some_and(|id| private.contains(id))
}
/// Opens a tab in the session's window and resolves to its target id. Chromium's own
/// `Target.createTarget` would open a separate window outside the app.
pub type TabOpener = std::sync::Arc<
    dyn Fn(String) -> std::pin::Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync,
>;
/// Told where an agent moved or clicked: target id, viewport position, and whether a button
/// went down. The window shows it, so the person sees what the agent is doing.
pub type InputObserver = std::sync::Arc<dyn Fn(&str, f64, f64, bool) + Send + Sync>;
/// What ties an agent's connection to the session's window.
#[derive(Default)]
pub struct WindowHooks {
    /// Targets an agent must never see, such as the app's own panel.
    pub private: HashSet<String>,
    pub opener: Option<TabOpener>,
    pub observer: Option<InputObserver>,
}
/// Browser-wide commands are default-denied. Flat page commands use an allow-list
/// of domains; experimental browser, IO, storage and extension domains stay private.
pub fn allowed(method: &str, attached: bool) -> bool {
    if attached {
        return method.split('.').next().is_some_and(|domain| {
            matches!(
                domain,
                "Page"
                    | "Runtime"
                    | "DOM"
                    | "CSS"
                    | "Accessibility"
                    | "Input"
                    | "Network"
                    | "Log"
                    | "Emulation"
                    | "Security"
                    | "Performance"
                    | "Debugger"
                    | "Overlay"
                    | "Fetch"
                    | "Inspector"
            )
        }) || matches!(method, "Target.getTargetInfo" | "Target.setAutoAttach");
    }
    matches!(
        method,
        "Target.attachToBrowserTarget"
            | "Browser.getVersion"
            | "Browser.getWindowForTarget"
            | "Browser.setWindowBounds"
            | "Target.getTargets"
            | "Target.getTargetInfo"
            | "Target.setDiscoverTargets"
            | "Target.setAutoAttach"
            | "Target.attachToTarget"
            | "Target.detachFromTarget"
            | "Target.createTarget"
            | "Target.closeTarget"
            | "Browser.setDownloadBehavior"
            | "Storage.getCookies"
            | "Storage.setCookies"
            | "Storage.clearCookies"
    )
}
#[derive(Clone)]
pub struct AgentAccess {
    pub agent_id: String,
    pub token: String,
    revoked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    validator: Option<std::sync::Arc<dyn Fn() -> bool + Send + Sync>>,
}
impl AgentAccess {
    pub fn new(agent_id: String, token: String) -> Self {
        Self {
            agent_id,
            token,
            revoked: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            validator: None,
        }
    }
    pub fn with_validator(mut self, validator: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.validator = Some(std::sync::Arc::new(validator));
        self
    }
    pub fn revoke(&self) {
        self.revoked
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn check(&self) -> Result<()> {
        if self.revoked.load(std::sync::atomic::Ordering::SeqCst)
            || self.validator.as_ref().is_some_and(|valid| !valid())
        {
            Err(Error::Denied("agent token revoked"))
        } else {
            Ok(())
        }
    }
}
/// An authenticated, session-bound accepted socket. The daemon must resolve
/// sessionId itself and pass its configured AgentAccess (never client claims).
pub(crate) async fn serve<S>(
    ws: WebSocketStream<S>,
    upstream: &str,
    auth: AgentAccess,
    authorization: Option<&str>,
    control: Arbitration,
    window: WindowHooks,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    authenticate(authorization, &auth.token)?;
    let WindowHooks {
        private,
        opener,
        observer,
    } = window;
    let private = std::sync::Arc::new(private);
    let cdp = Cdp::connect(upstream).await?;
    let mut events = cdp.subscribe();
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(256);
    let mut sessions = HashMap::<String, String>::new();
    let mut queues = HashMap::<String, tokio::sync::mpsc::Sender<(Command, Value, u64)>>::new();
    let mut pages = HashSet::<String>::new();
    let mut browser_sessions = HashSet::<String>::new();
    let mut event_parents = HashMap::<String, String>::new();
    // Whether the agent asked to be attached to new pages automatically.
    let mut auto_attach = false;
    let initial = cdp.call("Target.getTargets", json!({}), None).await?;
    if let Some(infos) = initial["targetInfos"].as_array() {
        for info in infos.iter().filter(|v| agent_page(v, &private)) {
            if let Some(id) = info["targetId"].as_str() {
                pages.insert(id.into());
            }
        }
    }
    loop {
        tokio::select! {
            outgoing=rx.recv()=>if let Some(value)=outgoing { sink.send(value.to_string().into()).await?; },
            incoming=stream.next()=> {
                let Some(message) = incoming else {
                    break;
                };
                let message = message?;
                if message.is_close() {
                    break;
                }
                if !message.is_text() {
                    continue;
                }
                let command: Command = serde_json::from_str(message.to_text()?)?;
                let target = command
                    .session_id
                    .as_ref()
                    .and_then(|s| sessions.get(s))
                    .cloned();
                let response_id = command.id;
                let response_session = command.session_id.clone();
                if let Err(error) = auth.check() {
                    sink.send(
                        reply(response_id, response_session, Err(error))
                            .to_string()
                            .into(),
                    )
                    .await?;
                    continue;
                }
                let mut params = command.params.clone();
                let method = command.method.as_str();
                let explicit = params["targetId"].as_str().map(str::to_owned);
                let denied = !allowed(method, target.is_some())
                    || (command.session_id.is_some()
                        && target.is_none()
                        && !command
                            .session_id
                            .as_ref()
                            .is_some_and(|s| browser_sessions.contains(s)))
                    || explicit.as_deref().is_some_and(|id| !pages.contains(id))
                    || (method == "Target.createTarget"
                        && (params.get("browserContextId").is_some()
                            || !page_target(&json!({"type":"page","url":params["url"]}))))
                    || (method == "Target.detachFromTarget"
                        && !params["sessionId"]
                            .as_str()
                            .is_some_and(|id| sessions.contains_key(id) || browser_sessions.contains(id)))
                    || (method == "Page.navigate" && !page_target(&json!({"type":"page","url":params["url"]})))
                    || (method == "Browser.setWindowBounds")
                    || params.get("browserContextId").is_some();
                if method=="Target.getTargetInfo" && target.is_none() && explicit.is_none() && !denied {
                    let result=control.check(&auth.agent_id).await.map(|_|json!({"targetInfo":{"targetId":"zuruf-virtual-browser","type":"browser","title":"Zuruf","url":"","attached":true,"canAccessOpener":false}}));
                    sink.send(reply(response_id,response_session,result).to_string().into()).await?;continue;
                }
                if method == "Target.attachToBrowserTarget" && !denied {
                    let result = control.check(&auth.agent_id).await.map(|_| {
                        let id = format!("zuruf-browser-{}", uuid::Uuid::now_v7());
                        browser_sessions.insert(id.clone());
                        json!({"sessionId":id})
                    });
                    sink.send(
                        reply(response_id, response_session, result)
                            .to_string()
                            .into(),
                    )
                    .await?;
                    continue;
                }
                if method == "Browser.setDownloadBehavior" && !denied {
                    let result = control.check(&auth.agent_id).await.map(|_| json!({}));
                    sink.send(
                        reply(response_id, response_session, result)
                            .to_string()
                            .into(),
                    )
                    .await?;
                    continue;
                }
                if denied {
                    sink.send(json!({"id":response_id,"sessionId":response_session,"error":{"code":-32000,"message":"Zuruf: target or command forbidden"}}).to_string().into()).await?;
                    continue;
                }
                if method == "Target.setAutoAttach" || method == "Target.setDiscoverTargets" {
                    if method == "Target.setAutoAttach" && target.is_none() {
                        auto_attach = params["autoAttach"] == true;
                    }
                    params["filter"] = json!([{"type":"page","exclude":false},{"exclude":true}]);
                    if method == "Target.setAutoAttach" {
                        params["flatten"] = json!(true);
                        params["waitForDebuggerOnStart"] = json!(false);
                    }
                }
                // Attach operations must be observed here before the next client command.
                if matches!(
                    method,
                    "Target.attachToTarget"
                        | "Target.createTarget"
                        | "Target.getTargets"
                        | "Target.getTargetInfo"
                        | "Target.setAutoAttach"
                        | "Target.setDiscoverTargets"
                ) {
                    let result = match control.check(&auth.agent_id).await {
                        Ok(_) if method == "Target.createTarget" && opener.is_some() => {
                            let url = params["url"].as_str().unwrap_or("about:blank").to_owned();
                            let opened = match &opener {
                                Some(open) => open(url).await,
                                None => Err(Error::Closed),
                            };
                            if let Ok(id) = &opened
                                && auto_attach
                            {
                                // A client that attaches automatically, such as Playwright,
                                // expects the page's attach event ahead of this reply.
                                let deadline =
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(3);
                                while let Ok(Ok(event)) =
                                    tokio::time::timeout_at(deadline, events.recv()).await
                                {
                                    let announced = event["method"] == "Target.attachedToTarget"
                                        && event["params"]["targetInfo"]["targetId"] == id.as_str();
                                    if let Some(event) = visible_event(
                                        event,
                                        &mut sessions,
                                        &mut pages,
                                        &event_parents,
                                        &private,
                                    ) {
                                        sink.send(event.to_string().into()).await?;
                                    }
                                    if announced {
                                        break;
                                    }
                                }
                            }
                            opened.map(|id| json!({"targetId": id}))
                        }
                        Ok(_) => {
                            cdp.call(
                                method,
                                params,
                                command
                                    .session_id
                                    .as_deref()
                                    .filter(|id| !id.starts_with("zuruf-browser-")),
                            )
                            .await
                        }
                        Err(e) => Err(e),
                    };
                    let result = result.map(|mut value| {
                        if method == "Target.getTargets"
                            && let Some(infos) = value["targetInfos"].as_array_mut()
                        {
                            infos.retain(|info| agent_page(info, &private));
                        }
                        if method == "Target.attachToTarget"
                            && let (Some(id), Some(target)) = (value["sessionId"].as_str(), explicit.as_deref())
                        {
                            sessions.insert(id.into(), target.into());
                            if let Some(parent) = &response_session {
                                event_parents.insert(id.into(), parent.clone());
                            }
                        }
                        if method == "Target.createTarget"
                            && let Some(id) = value["targetId"].as_str()
                        {
                            pages.insert(id.into());
                        }
                        value
                    });
                    sink.send(
                        reply(response_id, response_session, result)
                            .to_string()
                            .into(),
                    )
                    .await?;
                } else {
                    let generation = match control.check(&auth.agent_id).await {
                        Ok(generation) => generation,
                        Err(error) => {
                            sink.send(
                                reply(response_id, response_session, Err(error))
                                    .to_string()
                                    .into(),
                            )
                            .await?;
                            continue;
                        }
                    };
                    let tab = target.unwrap_or_else(|| explicit.as_deref().unwrap_or("browser").into());
                    let queue = queues.entry(tab.clone()).or_insert_with(|| {
                        let (queue, mut jobs) = tokio::sync::mpsc::channel::<(Command, Value, u64)>(256);
                        let backend = cdp.clone();
                        let replies = tx.clone();
                        let gate = control.clone();
                        let agent = auth.agent_id.clone();
                        let access = auth.clone();
                        let private = private.clone();
                        let observer = observer.clone();
                        tokio::spawn(async move {
                            while let Some((command, params, generation)) = jobs.recv().await {
                                if replies.is_closed() {
                                    break;
                                }
                                let result = async {
                                    access.check()?;
                                    let permit = gate.acquire(&agent, &tab).await?;
                                    if permit.generation != generation {
                                        return Err(Error::Denied("stale queued command"));
                                    }
                                    if tab!="browser" {
                                        let info=backend.call("Target.getTargetInfo",json!({"targetId":tab}),None).await?;
                                        if !agent_page(&info["targetInfo"], &private) {
                                            return Err(Error::Denied("target is no longer a page"));
                                        }
                                    }
                                    access.check()?;
                                    if gate.check(&agent).await?!=generation{return Err(Error::Denied("revoked before dispatch"));}
                                    if command.method == "Input.dispatchMouseEvent"
                                        && let Some(observer) = &observer
                                        && let (Some(x), Some(y)) =
                                            (params["x"].as_f64(), params["y"].as_f64())
                                    {
                                        observer(&tab, x, y, params["type"] == "mousePressed");
                                    }
                                    backend
                                        .call(
                                            &command.method,
                                            params,
                                            command
                                                .session_id
                                                .as_deref()
                                                .filter(|id| !id.starts_with("zuruf-browser-")),
                                        )
                                        .await
                                }
                                .await;
                                if replies
                                    .send(reply(command.id, command.session_id, result))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        });
                        queue
                    });
                    if queue.try_send((command, params, generation)).is_err() {
                        sink.send(
                            reply(
                                response_id,
                                response_session,
                                Err(Error::Denied("tab queue full")),
                            )
                            .to_string()
                            .into(),
                        )
                        .await?;
                    }
                }
            },
            event=events.recv()=> {
                let Ok(event) = event else {
                    break;
                };
                let Some(event) =
                    visible_event(event, &mut sessions, &mut pages, &event_parents, &private)
                else {
                    continue;
                };
                sink.send(event.to_string().into()).await?;
            }
        }
    }
    Ok(())
}
/// Decide whether a browser event may reach the agent, and remember what it reveals about
/// pages and sessions. Returns the event as the agent should see it.
fn visible_event(
    mut event: Value,
    sessions: &mut HashMap<String, String>,
    pages: &mut HashSet<String>,
    event_parents: &HashMap<String, String>,
    private: &HashSet<String>,
) -> Option<Value> {
    let method = event["method"].as_str().unwrap_or_default().to_owned();
    let info = event["params"]["targetInfo"].clone();
    if method == "Target.attachedToTarget" {
        if !agent_page(&info, private) {
            return None;
        }
        if let (Some(session), Some(target)) = (
            event["params"]["sessionId"].as_str(),
            info["targetId"].as_str(),
        ) {
            sessions.insert(session.into(), target.into());
            pages.insert(target.into());
        }
    } else if matches!(
        method.as_str(),
        "Target.targetCreated" | "Target.targetInfoChanged"
    ) {
        if !agent_page(&info, private) {
            if let Some(id) = info["targetId"].as_str() {
                pages.remove(id);
                sessions.retain(|_, target| target != id);
            }
            return None;
        }
        if let Some(id) = info["targetId"].as_str() {
            pages.insert(id.into());
        }
    } else if method == "Target.targetDestroyed" {
        if !event["params"]["targetId"]
            .as_str()
            .is_some_and(|id| pages.remove(id))
        {
            return None;
        }
    } else if method.starts_with("Target.") {
        if !event["params"]["sessionId"]
            .as_str()
            .is_some_and(|id| sessions.contains_key(id))
        {
            return None;
        }
    } else if !event["sessionId"]
        .as_str()
        .is_some_and(|id| sessions.contains_key(id))
    {
        return None;
    }
    if method == "Target.attachedToTarget"
        && let Some(parent) = event["params"]["sessionId"]
            .as_str()
            .and_then(|id| event_parents.get(id))
    {
        event["sessionId"] = json!(parent);
    }
    Some(event)
}
fn reply(id: u64, session: Option<String>, result: Result<Value>) -> Value {
    match result {
        Ok(result) => json!({"id":id,"sessionId":session,"result":result}),
        Err(e) => {
            json!({"id":id,"sessionId":session,"error":{"code":-32000,"message":e.to_string()}})
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters() {
        assert!(!allowed("Browser.close", false));
        assert!(!allowed("Target.createBrowserContext", false));
        assert!(!allowed("Runtime.evaluate", false));
        assert!(allowed("Runtime.evaluate", true));
        assert!(!page_target(
            &json!({"type":"page","url":"chrome-extension://id/panel.html"})
        ));
        assert!(!page_target(
            &json!({"type":"service_worker","url":"https://example.com"})
        ));
        assert!(authenticate(None, "secret").is_err());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::{sync::Arc, time::Duration};
    use tokio::{net::TcpListener, sync::Mutex};
    #[tokio::test]
    async fn mock_backend_preserves_mouse_order_and_revokes() {
        let remote = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", remote.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let recorded = seen.clone();
        let backend = tokio::spawn(async move {
            let (stream, _) = remote.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = ws.next().await {
                if !message.is_text() {
                    continue;
                }
                let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                let method = value["method"].as_str().unwrap();
                let result = match method {
                    "Target.getTargets" => {
                        json!({"targetInfos":[{"type":"page","url":"https://fixture/","targetId":"page"},{"type":"page","url":"chrome-extension://private/panel.html","targetId":"panel"}]})
                    }
                    "Target.attachToTarget" => json!({"sessionId":"flat"}),
                    "Target.getTargetInfo" => {
                        json!({"targetInfo":{"type":"page","url":"https://fixture/","targetId":"page"}})
                    }
                    "Input.dispatchMouseEvent" => {
                        recorded
                            .lock()
                            .await
                            .push(value["params"]["type"].as_str().unwrap().into());
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        json!({})
                    }
                    _ => json!({}),
                };
                ws.send(json!({"id":value["id"],"result":result}).to_string().into())
                    .await
                    .unwrap();
            }
        });
        let local = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}", local.local_addr().unwrap());
        let control = Arbitration::default();
        control
            .grant("test".into(), Duration::from_secs(10))
            .await
            .unwrap();
        let gate = control.clone();
        let proxy = tokio::spawn(async move {
            let (stream, _) = local.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve(
                ws,
                &endpoint,
                AgentAccess::new("test".into(), "test".into()),
                Some("Bearer test"),
                gate,
                WindowHooks::default(),
            )
            .await
        });
        let (mut client, _) = tokio_tungstenite::connect_async(address).await.unwrap();
        client
            .send(
                json!({"id":1,"method":"Target.attachToTarget","params":{"targetId":"panel"}})
                    .to_string()
                    .into(),
            )
            .await
            .unwrap();
        let denied: Value =
            serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert!(denied.get("error").is_some());
        client
            .send(
                json!({"id":2,"method":"Target.attachToTarget","params":{"targetId":"page"}})
                    .to_string()
                    .into(),
            )
            .await
            .unwrap();
        let _ = client.next().await;
        for (id, kind) in [(3, "mousePressed"), (4, "mouseReleased")] {
            client.send(json!({"id":id,"sessionId":"flat","method":"Input.dispatchMouseEvent","params":{"type":kind}}).to_string().into()).await.unwrap();
        }
        let _ = client.next().await;
        let _ = client.next().await;
        assert_eq!(*seen.lock().await, vec!["mousePressed", "mouseReleased"]);
        control.begin_takeover().await;
        client.send(json!({"id":5,"sessionId":"flat","method":"Runtime.evaluate","params":{"expression":"mutate()"}}).to_string().into()).await.unwrap();
        let denied: Value =
            serde_json::from_str(client.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert!(denied.get("error").is_some());
        client.close(None).await.unwrap();
        proxy.abort();
        backend.abort();
    }
}
