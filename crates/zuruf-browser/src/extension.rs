use crate::{Binding, ControlState, DomCapture, Error, Point, Rect, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, broadcast, mpsc, oneshot},
};
use tokio_tungstenite::WebSocketStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub port: u16,
    pub session_id: String,
    pub token: String,
    pub protocol_version: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub v: u32,
    pub request_id: String,
    pub session_id: String,
    #[serde(default)]
    pub tab_id: Option<i64>,
    #[serde(default)]
    pub frame_id: Option<i64>,
    #[serde(default)]
    pub document_id: Option<String>,
    pub timestamp: u64,
    #[serde(flatten)]
    pub message: Message,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Message {
    #[serde(rename = "hello")]
    Hello,
    #[serde(rename = "resume")]
    Resume,
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(rename = "ack")]
    Ack {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "configure")]
    Configure {
        review: bool,
        bindings: Vec<Binding>,
    },
    #[serde(rename = "pointer")]
    Pointer { cursor: Point },
    #[serde(rename = "action")]
    Action { action: String },
    #[serde(rename = "capture.request")]
    Capture {
        kind: String,
        cursor: Point,
        rect: Option<Rect>,
        dom: Option<DomCapture>,
    },
    #[serde(rename = "overlay.hide")]
    Hide,
    #[serde(rename = "overlay.hidden")]
    Hidden {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "overlay.show")]
    Show,
    #[serde(rename = "capture.saved")]
    Saved,
    #[serde(rename = "capture.failed")]
    Failed { reason: String },
    #[serde(rename = "control.changed")]
    Control(ControlState),
    #[serde(rename = "tabs")]
    Tabs { tabs: Vec<ExtensionTab> },
    #[serde(rename = "dom.request")]
    DomRequest,
    #[serde(rename = "dom.result")]
    DomResult {
        #[serde(rename = "requestId")]
        request_id: String,
        dom: Option<DomCapture>,
        cursor: Point,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionTab {
    pub tab_id: i64,
    pub target_id: String,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub scriptable: bool,
}
#[derive(Clone)]
pub struct ExtensionHub {
    pub bootstrap: Bootstrap,
    inner: Arc<Mutex<State>>,
    events: broadcast::Sender<Envelope>,
}
#[derive(Default)]
struct State {
    sender: Option<mpsc::Sender<Envelope>>,
    seen: HashSet<String>,
    order: VecDeque<String>,
    waiting: HashMap<String, oneshot::Sender<Envelope>>,
    review: bool,
    bindings: Vec<Binding>,
    pub tabs: Vec<ExtensionTab>,
    pub pointers: HashMap<i64, (i64, Point)>,
}
impl ExtensionHub {
    pub fn new(bootstrap: Bootstrap) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            bootstrap,
            inner: Arc::default(),
            events,
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.events.subscribe()
    }
    pub async fn tabs(&self) -> Vec<ExtensionTab> {
        self.inner.lock().await.tabs.clone()
    }
    pub async fn pointer(&self, tab: i64) -> (i64, Point) {
        self.inner
            .lock()
            .await
            .pointers
            .get(&tab)
            .copied()
            .unwrap_or_default()
    }
    pub fn envelope(
        &self,
        message: Message,
        tab_id: Option<i64>,
        frame_id: Option<i64>,
    ) -> Envelope {
        Envelope {
            v: 1,
            request_id: uuid::Uuid::now_v7().to_string(),
            session_id: self.bootstrap.session_id.clone(),
            tab_id,
            frame_id,
            document_id: None,
            timestamp: 0,
            message,
        }
    }
    pub async fn send(&self, message: Message, tab: Option<i64>, frame: Option<i64>) -> Result<()> {
        let sender = self
            .inner
            .lock()
            .await
            .sender
            .clone()
            .ok_or(Error::Closed)?;
        sender
            .send(self.envelope(message, tab, frame))
            .await
            .map_err(|_| Error::Closed)
    }
    pub async fn request(
        &self,
        message: Message,
        tab: i64,
        frame: Option<i64>,
    ) -> Result<Envelope> {
        let envelope = self.envelope(message, Some(tab), frame);
        let id = envelope.request_id.clone();
        let (tx, rx) = oneshot::channel();
        let sender = {
            let mut state = self.inner.lock().await;
            let sender = state.sender.clone().ok_or(Error::Closed)?;
            state.waiting.insert(id.clone(), tx);
            sender
        };
        if sender.send(envelope).await.is_err() {
            self.inner.lock().await.waiting.remove(&id);
            return Err(Error::Closed);
        }
        let result = tokio::time::timeout(Duration::from_secs(3), rx).await;
        self.inner.lock().await.waiting.remove(&id);
        result
            .map_err(|_| Error::Timeout("extension acknowledgement"))?
            .map_err(|_| Error::Closed)
    }
    pub async fn configure(&self, review: bool, bindings: Vec<Binding>) -> Result<()> {
        {
            let mut s = self.inner.lock().await;
            s.review = review;
            s.bindings = bindings.clone();
        }
        // Configuration is retained even if the worker is currently asleep.
        match self
            .send(Message::Configure { review, bindings }, None, None)
            .await
        {
            Err(Error::Closed) => Ok(()),
            result => result,
        }
    }
    /// `origin` must be the HTTP upgrade header. `extension_id` comes from trusted
    /// installation configuration, never from the client's hello payload.
    pub async fn serve<S>(
        &self,
        mut ws: WebSocketStream<S>,
        origin: &str,
        extension_id: &str,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        if extension_id.is_empty() || origin != format!("chrome-extension://{extension_id}") {
            return Err(Error::Denied("extension origin"));
        }
        let first = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .map_err(|_| Error::Timeout("extension auth"))?
            .ok_or(Error::Closed)??;
        let auth: serde_json::Value = serde_json::from_str(first.to_text()?)?;
        if auth["type"] != "auth" || auth["token"] != self.bootstrap.token {
            return Err(Error::Denied("extension token"));
        }
        let (tx, mut rx) = mpsc::channel(128);
        {
            let mut state = self.inner.lock().await;
            state.sender = Some(tx);
        }
        let configure = {
            let s = self.inner.lock().await;
            Message::Configure {
                review: s.review,
                bindings: s.bindings.clone(),
            }
        };
        ws.send(serde_json::to_string(&self.envelope(configure, None, None))?.into())
            .await?;
        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        let mut last_seen = tokio::time::Instant::now();
        loop {
            tokio::select! {
                outgoing=rx.recv()=>{let Some(outgoing)=outgoing else{break;};ws.send(serde_json::to_string(&outgoing)?.into()).await?;},
                _=heartbeat.tick()=>{if last_seen.elapsed()>Duration::from_secs(45){return Err(Error::Timeout("extension heartbeat"));}ws.send(serde_json::to_string(&self.envelope(Message::Heartbeat,None,None))?.into()).await?;},
                incoming=ws.next()=>{
                    let Some(incoming)=incoming else{break;};let incoming=incoming?;if incoming.is_close(){break;}
                if !incoming.is_text(){continue;}
                    let envelope:Envelope=serde_json::from_str(incoming.to_text()?)?;
                    if envelope.v!=1 || envelope.session_id!=self.bootstrap.session_id{return Err(Error::Denied("extension session/version"));}
                    last_seen=tokio::time::Instant::now();
                    if matches!(envelope.message,Message::Ack{..}|Message::Heartbeat){continue;}
                    let mut state=self.inner.lock().await;
                    let duplicate=!state.seen.insert(envelope.request_id.clone());
                    if !duplicate {
                        state.order.push_back(envelope.request_id.clone());if state.order.len()>4096 && let Some(old)=state.order.pop_front(){state.seen.remove(&old);}
                        match &envelope.message {
                            Message::Hidden{request_id}|Message::DomResult{request_id,..}=>{if let Some(reply)=state.waiting.remove(request_id){let _=reply.send(envelope.clone());}},
                            Message::Tabs{tabs}=>state.tabs=tabs.clone(),
                            Message::Pointer{cursor}=>{if let(Some(tab),Some(frame))=(envelope.tab_id,envelope.frame_id){state.pointers.insert(tab,(frame,*cursor));}},
                            _=>{},
                        }
                        let _=self.events.send(envelope.clone());
                    }
                    drop(state);
                    ws.send(serde_json::to_string(&self.envelope(Message::Ack{request_id:envelope.request_id},None,None))?.into()).await?;
                }
            }
        }
        Ok(())
    }
}
