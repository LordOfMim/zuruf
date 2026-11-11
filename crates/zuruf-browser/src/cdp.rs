//! Small multiplexed JSON-RPC transport; flat CDP session IDs are explicit.
use crate::{Error, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
#[derive(Clone)]
pub struct Cdp {
    tx: mpsc::Sender<Value>,
    pending: Pending,
    next: Arc<AtomicU64>,
    events: broadcast::Sender<Value>,
}
impl Cdp {
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(endpoint).await?;
        let (mut sink, mut stream) = ws.split();
        let (tx, mut rx) = mpsc::channel::<Value>(256);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        // Generous: the page bridge must not miss execution-context events during a burst.
        let (events, _) = broadcast::channel(4096);
        let client = Self {
            tx,
            pending: pending.clone(),
            next: Arc::new(AtomicU64::new(1)),
            events: events.clone(),
        };
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    out = rx.recv() => match out {
                        Some(value) => if sink.send(value.to_string().into()).await.is_err() { break; },
                        None => break,
                    },
                    incoming = stream.next() => match incoming {
                        Some(Ok(message)) if message.is_text() => {
                            let Ok(value) = serde_json::from_str::<Value>(&message.to_string()) else { continue; };
                            if let Some(id) = value["id"].as_u64() {
                                if let Some(reply) = pending.lock().await.remove(&id) {
                                    let result = if value.get("error").is_some() { Err(Error::Cdp(value["error"].clone())) } else { Ok(value["result"].clone()) };
                                    let _ = reply.send(result);
                                }
                            } else { let _ = events.send(value); }
                        },
                        Some(Ok(message)) if message.is_ping() => { let _ = sink.send(tokio_tungstenite::tungstenite::Message::Pong(message.into_data())).await; },
                        Some(Ok(message)) if message.is_close() => break,
                        Some(Ok(_)) => {},
                        _ => break,
                    }
                }
            }
            for (_, reply) in pending.lock().await.drain() {
                let _ = reply.send(Err(Error::Closed));
            }
            let _ = events.send(json!({"method":"Zuruf.disconnected"}));
        });
        Ok(client)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub async fn call(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        self.call_timeout(method, params, session, Duration::from_secs(30))
            .await
    }
    pub(crate) async fn call_timeout(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let cmd = Command {
            id,
            method: method.into(),
            params,
            session_id: session.map(str::to_owned),
        };
        let result = tokio::time::timeout(timeout, async {
            self.tx
                .send(serde_json::to_value(cmd)?)
                .await
                .map_err(|_| Error::Closed)?;
            rx.await.map_err(|_| Error::Closed)?
        })
        .await;
        self.pending.lock().await.remove(&id);
        result.map_err(|_| Error::Timeout("CDP reply"))?
    }
    pub async fn attach(&self, target: &str) -> Result<String> {
        let result = self
            .call(
                "Target.attachToTarget",
                json!({"targetId":target,"flatten":true}),
                None,
            )
            .await?;
        result["sessionId"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Invalid("missing sessionId".into()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing() {
        let command = Command {
            id: 7,
            method: "Page.enable".into(),
            params: json!({}),
            session_id: Some("flat".into()),
        };
        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["sessionId"], "flat");
        assert_eq!(value["id"], 7);
        assert!(value.get("jsonrpc").is_none());
    }
}
