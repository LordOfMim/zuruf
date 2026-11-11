//! The page side of a session, without a browser extension.
//!
//! Zuruf's page agent (`apps/page`) is injected over CDP into an isolated world of every frame.
//! It reports key presses, the pointer and picker results through a binding that only exists in
//! that world, so web pages can neither see nor call it. Commands travel the other way by
//! evaluating a function in the same world.
use crate::{Binding, ControlState, DomCapture, Error, Point, Rect, Result, cdp::Cdp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Mutex, broadcast};

/// Name of the isolated world the agent lives in.
const WORLD: &str = "zuruf";
/// Function the agent calls to reach the app. Exposed in [`WORLD`] only.
const BINDING: &str = "__zurufSend";
/// Function the app calls to reach the agent.
const RECEIVER: &str = "__zurufReceive";
const AGENT: &str = include_str!(concat!(env!("OUT_DIR"), "/agent.js"));

/// A message from one document of one tab, with the identity the app assigned to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub request_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<i64>,
    /// Changes with every document a frame loads; captures are bound to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(flatten)]
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Message {
    #[serde(rename = "frame.ready")]
    FrameReady,
    #[serde(rename = "configure")]
    Configure {
        review: bool,
        bindings: Vec<Binding>,
    },
    #[serde(rename = "action")]
    Action { action: String },
    #[serde(rename = "capture.request")]
    Capture {
        kind: String,
        cursor: Point,
        rect: Option<Rect>,
        dom: Option<Box<DomCapture>>,
    },
    #[serde(rename = "overlay.hide")]
    Hide,
    #[serde(rename = "overlay.show")]
    Show,
    #[serde(rename = "capture.saved")]
    Saved,
    #[serde(rename = "capture.failed")]
    Failed { reason: String },
    #[serde(rename = "control.changed")]
    Control(ControlState),
    #[serde(rename = "status")]
    Status { recording: bool },
    #[serde(rename = "tabs")]
    Tabs { tabs: Vec<PageTab> },
    #[serde(rename = "dom.request")]
    DomRequest,
}

/// What the agent sends through the binding.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload")]
enum FromPage {
    #[serde(rename = "ready")]
    Ready { top: bool, visible: bool },
    #[serde(rename = "visibility")]
    Visibility { visible: bool },
    #[serde(rename = "title")]
    Title { title: String },
    #[serde(rename = "pointer")]
    Pointer { cursor: Point },
    #[serde(rename = "action")]
    Action { action: String },
    #[serde(rename = "capture.request")]
    Capture {
        kind: String,
        cursor: Point,
        rect: Option<Rect>,
        dom: Option<Box<DomCapture>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTab {
    pub tab_id: i64,
    pub target_id: String,
    pub url: String,
    pub title: String,
    /// The tab the user currently sees.
    pub active: bool,
    /// The agent runs in the tab's current top document.
    pub scriptable: bool,
}

/// The agent's world in one frame's current document.
#[derive(Debug, Clone)]
struct Frame {
    number: i64,
    session: String,
    context: i64,
    document: String,
}

#[derive(Debug, Default)]
struct Target {
    tab: i64,
    url: String,
    title: String,
    visible: bool,
    /// CDP session attached to the page itself.
    session: Option<String>,
    /// Keyed by CDP frame id.
    frames: HashMap<String, Frame>,
    /// Stable small numbers per CDP frame id; the top frame is always 0.
    numbers: HashMap<String, i64>,
}

#[derive(Default)]
struct State {
    review: bool,
    bindings: Vec<Binding>,
    recording: bool,
    control: Option<ControlState>,
    next_tab: i64,
    /// Keyed by CDP target id of the page.
    targets: HashMap<String, Target>,
    /// CDP session id → target id of the page it belongs to (itself or an embedded frame).
    sessions: HashMap<String, String>,
    /// Targets that are never tabs, such as the app's own panel.
    private: HashSet<String>,
    pointers: HashMap<i64, (i64, Point)>,
}

impl State {
    fn tabs(&self) -> Vec<PageTab> {
        let mut tabs: Vec<PageTab> = self
            .targets
            .iter()
            .filter(|(_, target)| listed(&target.url))
            .map(|(id, target)| PageTab {
                tab_id: target.tab,
                target_id: id.clone(),
                url: target.url.clone(),
                title: target.title.clone(),
                active: target.visible,
                scriptable: target.frames.values().any(|frame| frame.number == 0),
            })
            .collect();
        tabs.sort_by_key(|tab| tab.tab_id);
        tabs
    }
    fn target_of_tab(&self, tab: i64) -> Option<&Target> {
        self.targets.values().find(|target| target.tab == tab)
    }
    /// Agent worlds to address: one frame, one tab or every tab.
    fn worlds(&self, tab: Option<i64>, frame: Option<i64>) -> Vec<(String, i64)> {
        self.targets
            .values()
            .filter(|target| tab.is_none_or(|tab| target.tab == tab))
            .flat_map(|target| target.frames.values())
            .filter(|world| frame.is_none_or(|frame| world.number == frame))
            .map(|world| (world.session.clone(), world.context))
            .collect()
    }
}

/// Browser-internal pages are not review tabs.
fn listed(url: &str) -> bool {
    !url.starts_with("devtools://") && !url.starts_with("chrome-extension://")
}

#[derive(Clone)]
pub struct PageBridge {
    session_id: String,
    cdp: Cdp,
    state: Arc<Mutex<State>>,
    events: broadcast::Sender<Envelope>,
}

impl PageBridge {
    /// Attach to every page of the browser behind `cdp`, now and in the future.
    /// `private` lists targets that must never be treated as tabs.
    pub async fn start(cdp: Cdp, session_id: String, private: HashSet<String>) -> Result<Self> {
        let (events, _) = broadcast::channel(256);
        let bridge = Self {
            session_id,
            cdp: cdp.clone(),
            state: Arc::new(Mutex::new(State {
                private,
                ..State::default()
            })),
            events,
        };
        // Subscribe before enabling anything, so no attach event is missed.
        let mut incoming = cdp.subscribe();
        let worker = bridge.clone();
        tokio::spawn(async move {
            loop {
                match incoming.recv().await {
                    Ok(event) => worker.handle(event).await,
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "page bridge fell behind browser events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        cdp.call("Target.setDiscoverTargets", json!({"discover": true}), None)
            .await?;
        cdp.call(
            "Target.setAutoAttach",
            json!({
                "autoAttach": true,
                "waitForDebuggerOnStart": false,
                "flatten": true,
                "filter": [{"type": "page", "exclude": false}, {"exclude": true}],
            }),
            None,
        )
        .await?;
        Ok(bridge)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.events.subscribe()
    }
    pub async fn tabs(&self) -> Vec<PageTab> {
        self.state.lock().await.tabs()
    }
    /// Frame the pointer was last seen in, and its position in top-frame coordinates.
    pub async fn pointer(&self, tab: i64) -> (i64, Point) {
        self.state
            .lock()
            .await
            .pointers
            .get(&tab)
            .copied()
            .unwrap_or_default()
    }
    /// Identity of the document a frame currently shows.
    pub async fn document(&self, tab: i64, frame: i64) -> Option<String> {
        let state = self.state.lock().await;
        state
            .target_of_tab(tab)?
            .frames
            .values()
            .find(|world| world.number == frame)
            .map(|world| world.document.clone())
    }
    /// CDP session attached to a tab's page.
    pub async fn page_session(&self, target: &str) -> Option<String> {
        self.state.lock().await.targets.get(target)?.session.clone()
    }
    pub async fn is_private(&self, target: &str) -> bool {
        self.state.lock().await.private.contains(target)
    }

    pub async fn configure(&self, review: bool, bindings: Vec<Binding>) {
        {
            let mut state = self.state.lock().await;
            state.review = review;
            state.bindings = bindings.clone();
        }
        self.send(Message::Configure { review, bindings }, None, None)
            .await;
    }
    pub async fn push_bindings(&self, bindings: Vec<Binding>) {
        let review = self.state.lock().await.review;
        self.configure(review, bindings).await;
    }

    /// Deliver to one frame, one tab or every tab. Recording state and control owner are kept,
    /// so documents loaded later start with the current values.
    pub async fn send(&self, message: Message, tab: Option<i64>, frame: Option<i64>) {
        let worlds = {
            let mut state = self.state.lock().await;
            match &message {
                Message::Status { recording } => state.recording = *recording,
                Message::Control(control) => state.control = Some(control.clone()),
                _ => {}
            }
            state.worlds(tab, frame)
        };
        let deliveries = worlds
            .iter()
            .map(|(session, context)| self.evaluate(session, *context, &message));
        for result in futures_util::future::join_all(deliveries).await {
            // A document that went away while we were sending no longer needs the message.
            if let Err(error) = result {
                tracing::debug!(%error, "page message not delivered");
            }
        }
    }

    /// Hide the overlay in every frame of a tab. Fails unless all of them confirmed,
    /// so a screenshot never shows a half-hidden overlay.
    pub async fn hide(&self, tab: i64) -> Result<()> {
        let worlds = self.state.lock().await.worlds(Some(tab), None);
        let deliveries = worlds
            .iter()
            .map(|(session, context)| self.evaluate(session, *context, &Message::Hide));
        futures_util::future::try_join_all(deliveries).await?;
        Ok(())
    }

    /// Describe the element under the pointer in one frame.
    pub async fn dom(&self, tab: i64, frame: i64) -> Result<(Option<DomCapture>, Point)> {
        let world = self
            .state
            .lock()
            .await
            .worlds(Some(tab), Some(frame))
            .into_iter()
            .next()
            .ok_or(Error::Denied("frame is not scriptable"))?;
        let value = self
            .evaluate(&world.0, world.1, &Message::DomRequest)
            .await?;
        let dom = serde_json::from_value(value["dom"].clone())?;
        let cursor = serde_json::from_value(value["cursor"].clone()).unwrap_or_default();
        Ok((dom, cursor))
    }

    async fn evaluate(&self, session: &str, context: i64, message: &Message) -> Result<Value> {
        let expression = format!("globalThis.{RECEIVER}({})", serde_json::to_string(message)?);
        let value = self
            .cdp
            .call(
                "Runtime.evaluate",
                json!({"expression": expression, "contextId": context, "returnByValue": true}),
                Some(session),
            )
            .await?;
        if value.get("exceptionDetails").is_some() {
            return Err(Error::Cdp(value));
        }
        Ok(value["result"]["value"].clone())
    }

    fn envelope(&self, message: Message) -> Envelope {
        Envelope {
            request_id: uuid::Uuid::now_v7().to_string(),
            session_id: self.session_id.clone(),
            tab_id: None,
            frame_id: None,
            document_id: None,
            message,
        }
    }
    async fn announce_tabs(&self) {
        let tabs = self.state.lock().await.tabs();
        let _ = self.events.send(self.envelope(Message::Tabs { tabs }));
    }

    async fn handle(&self, event: Value) {
        let params = &event["params"];
        let session = event["sessionId"].as_str();
        match event["method"].as_str().unwrap_or_default() {
            "Target.attachedToTarget" => self.attached(session, params).await,
            "Target.detachedFromTarget" => {
                let Some(detached) = params["sessionId"].as_str() else {
                    return;
                };
                let mut state = self.state.lock().await;
                if let Some(target) = state.sessions.remove(detached)
                    && let Some(target) = state.targets.get_mut(&target)
                {
                    target.frames.retain(|_, world| world.session != detached);
                }
            }
            "Target.targetInfoChanged" => {
                let info = &params["targetInfo"];
                let Some(id) = info["targetId"].as_str() else {
                    return;
                };
                let changed = {
                    let mut state = self.state.lock().await;
                    match state.targets.get_mut(id) {
                        Some(target) => {
                            // The title comes from the page agent; the browser only reports a
                            // placeholder here and is silent about later changes.
                            let url = info["url"].as_str().unwrap_or_default();
                            let changed = target.url != url;
                            target.url = url.into();
                            if target.frames.is_empty() {
                                target.title = info["title"].as_str().unwrap_or_default().into();
                            }
                            changed
                        }
                        None => false,
                    }
                };
                if changed {
                    self.announce_tabs().await;
                }
            }
            "Target.targetDestroyed" => {
                let Some(id) = params["targetId"].as_str() else {
                    return;
                };
                let known = {
                    let mut state = self.state.lock().await;
                    state.sessions.retain(|_, target| target != id);
                    match state.targets.remove(id) {
                        Some(target) => {
                            state.pointers.remove(&target.tab);
                            true
                        }
                        None => false,
                    }
                };
                if known {
                    self.announce_tabs().await;
                }
            }
            "Runtime.executionContextCreated" => {
                let context = &params["context"];
                if context["name"] != WORLD {
                    return;
                }
                let (Some(session), Some(id), Some(frame)) = (
                    session,
                    context["id"].as_i64(),
                    context["auxData"]["frameId"].as_str(),
                ) else {
                    return;
                };
                let mut state = self.state.lock().await;
                let Some(target_id) = state.sessions.get(session).cloned() else {
                    return;
                };
                let Some(target) = state.targets.get_mut(&target_id) else {
                    return;
                };
                // The top frame of a page carries the page's target id.
                let next = target.numbers.len() as i64 + 1;
                let number = if frame == target_id {
                    0
                } else {
                    *target.numbers.entry(frame.into()).or_insert(next)
                };
                target.frames.insert(
                    frame.into(),
                    Frame {
                        number,
                        session: session.into(),
                        context: id,
                        document: context["uniqueId"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{session}:{id}")),
                    },
                );
            }
            "Runtime.executionContextDestroyed" => {
                let (Some(session), Some(id)) = (session, params["executionContextId"].as_i64())
                else {
                    return;
                };
                self.forget_worlds(session, Some(id)).await;
            }
            "Runtime.executionContextsCleared" => {
                if let Some(session) = session {
                    self.forget_worlds(session, None).await;
                }
            }
            "Runtime.bindingCalled" => {
                if params["name"] != BINDING {
                    return;
                }
                let (Some(session), Some(context), Some(payload)) = (
                    session,
                    params["executionContextId"].as_i64(),
                    params["payload"].as_str(),
                ) else {
                    return;
                };
                match serde_json::from_str::<FromPage>(payload) {
                    Ok(message) => self.page_message(session, context, message).await,
                    Err(error) => tracing::debug!(%error, "unreadable page message"),
                }
            }
            _ => {}
        }
    }

    async fn forget_worlds(&self, session: &str, context: Option<i64>) {
        let top_gone = {
            let mut state = self.state.lock().await;
            let Some(target_id) = state.sessions.get(session).cloned() else {
                return;
            };
            let Some(target) = state.targets.get_mut(&target_id) else {
                return;
            };
            let had_top = target.frames.values().any(|world| world.number == 0);
            target.frames.retain(|_, world| {
                world.session != session || context.is_some_and(|id| world.context != id)
            });
            had_top && !target.frames.values().any(|world| world.number == 0)
        };
        if top_gone {
            self.announce_tabs().await;
        }
    }

    async fn attached(&self, parent: Option<&str>, params: &Value) {
        let info = &params["targetInfo"];
        let (Some(session), Some(target_id)) =
            (params["sessionId"].as_str(), info["targetId"].as_str())
        else {
            return;
        };
        let page = info["type"] == "page";
        let owner = {
            let mut state = self.state.lock().await;
            if page {
                if state.private.contains(target_id) {
                    None
                } else {
                    state.next_tab += 1;
                    let tab = state.next_tab;
                    let target = state.targets.entry(target_id.into()).or_default();
                    if target.tab == 0 {
                        target.tab = tab;
                    }
                    target.url = info["url"].as_str().unwrap_or_default().into();
                    target.title = info["title"].as_str().unwrap_or_default().into();
                    target.session = Some(session.into());
                    Some(target_id.to_owned())
                }
            } else {
                // An embedded frame in its own process belongs to the page it was found from.
                parent.and_then(|parent| state.sessions.get(parent).cloned())
            }
        };
        let Some(owner) = owner else {
            let _ = self
                .cdp
                .call(
                    "Target.detachFromTarget",
                    json!({"sessionId": session}),
                    None,
                )
                .await;
            return;
        };
        self.state
            .lock()
            .await
            .sessions
            .insert(session.into(), owner);
        if page {
            self.announce_tabs().await;
        }
        let bridge = self.clone();
        let session = session.to_owned();
        tokio::spawn(async move {
            if let Err(error) = bridge.prepare(&session).await {
                tracing::debug!(%error, "page went away before the agent was installed");
            }
        });
    }

    /// Install the agent in a freshly attached page or embedded frame.
    async fn prepare(&self, session: &str) -> Result<()> {
        let session = Some(session);
        // Without the Page domain Chromium registers the script but never runs it in
        // documents loaded later.
        self.cdp.call("Page.enable", json!({}), session).await?;
        self.cdp.call("Runtime.enable", json!({}), session).await?;
        self.cdp
            .call(
                "Runtime.addBinding",
                json!({"name": BINDING, "executionContextName": WORLD}),
                session,
            )
            .await?;
        self.cdp
            .call(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"source": AGENT, "worldName": WORLD, "runImmediately": true}),
                session,
            )
            .await?;
        // Frames rendered in another process are separate targets; follow them too.
        self.cdp
            .call(
                "Target.setAutoAttach",
                json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": false,
                    "flatten": true,
                    "filter": [{"type": "iframe", "exclude": false}, {"exclude": true}],
                }),
                session,
            )
            .await?;
        Ok(())
    }

    /// Take in what a page agent sent through the binding.
    async fn page_message(&self, session: &str, context: i64, message: FromPage) {
        let (tab, frame, document, top_changed) = {
            let mut state = self.state.lock().await;
            let Some(target_id) = state.sessions.get(session).cloned() else {
                return;
            };
            let Some(target) = state.targets.get_mut(&target_id) else {
                return;
            };
            let Some(world) = target
                .frames
                .values()
                .find(|world| world.session == session && world.context == context)
                .cloned()
            else {
                return;
            };
            let tab = target.tab;
            let mut changed = false;
            match &message {
                FromPage::Ready { top, visible } => {
                    if *top {
                        target.visible = *visible;
                        changed = true;
                    }
                }
                FromPage::Visibility { visible } => {
                    changed = target.visible != *visible;
                    target.visible = *visible;
                }
                FromPage::Title { title } => {
                    // Only the top document names the tab.
                    if world.number == 0 {
                        changed = target.title != *title;
                        target.title = title.clone();
                    }
                }
                FromPage::Pointer { cursor } => {
                    state.pointers.insert(tab, (world.number, *cursor));
                }
                _ => {}
            }
            (tab, world.number, world.document, changed)
        };
        if top_changed {
            self.announce_tabs().await;
        }
        let forwarded = match message {
            FromPage::Ready { .. } => {
                self.greet(session, context).await;
                Message::FrameReady
            }
            FromPage::Action { action } => Message::Action { action },
            FromPage::Capture {
                kind,
                cursor,
                rect,
                dom,
            } => Message::Capture {
                kind,
                cursor,
                rect,
                dom,
            },
            FromPage::Visibility { .. } | FromPage::Title { .. } | FromPage::Pointer { .. } => {
                return;
            }
        };
        let mut envelope = self.envelope(forwarded);
        envelope.tab_id = Some(tab);
        envelope.frame_id = Some(frame);
        envelope.document_id = Some(document);
        let _ = self.events.send(envelope);
    }

    /// Bring a new document up to date.
    async fn greet(&self, session: &str, context: i64) {
        let (configure, recording, control) = {
            let state = self.state.lock().await;
            (
                Message::Configure {
                    review: state.review,
                    bindings: state.bindings.clone(),
                },
                state.recording,
                state.control.clone(),
            )
        };
        let mut messages = vec![configure, Message::Status { recording }];
        if let Some(control) = control {
            messages.push(Message::Control(control));
        }
        for message in messages {
            if self.evaluate(session, context, &message).await.is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_messages_parse() {
        let ready: FromPage =
            serde_json::from_str(r#"{"type":"ready","payload":{"top":true,"visible":false}}"#)
                .unwrap();
        assert!(matches!(
            ready,
            FromPage::Ready {
                top: true,
                visible: false
            }
        ));
        let capture: FromPage = serde_json::from_str(
            r#"{"type":"capture.request","payload":{"kind":"viewport","cursor":{"x":1,"y":2},"rect":null,"dom":null}}"#,
        )
        .unwrap();
        assert!(matches!(capture, FromPage::Capture { rect: None, .. }));
    }

    #[test]
    fn app_messages_use_the_agent_vocabulary() {
        assert_eq!(
            serde_json::to_value(Message::Hide).unwrap(),
            json!({"type":"overlay.hide"})
        );
        assert_eq!(
            serde_json::to_value(Message::Status { recording: true }).unwrap(),
            json!({"type":"status","payload":{"recording":true}})
        );
    }

    #[test]
    fn internal_pages_are_not_tabs() {
        assert!(listed("https://example.org/"));
        assert!(listed("about:blank"));
        assert!(!listed("devtools://devtools/bundled/inspector.html"));
    }
}
