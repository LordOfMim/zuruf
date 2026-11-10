use crate::{ControlState, Error, Result};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Default)]
pub struct Arbitration {
    inner: Arc<Mutex<State>>,
    tabs: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}
#[derive(Default)]
struct State {
    agent: Option<String>,
    until: Option<Instant>,
    human: bool,
    settling: bool,
    generation: u64,
}
pub struct Permit {
    pub generation: u64,
    _tab: OwnedMutexGuard<()>,
}
impl Arbitration {
    pub async fn grant(&self, agent: String, lease: Duration) -> Result<()> {
        let mut state = self.inner.lock().await;
        if state.human || state.settling {
            return Err(Error::Denied("human must release control first"));
        }
        state.generation += 1;
        state.agent = Some(agent);
        state.until = Some(Instant::now() + lease);
        Ok(())
    }
    /// Extend only a live lease owned by this agent, under the same lock as takeover.
    pub async fn renew(&self, agent: &str, lease: Duration) -> Result<()> {
        let mut s = self.inner.lock().await;
        if s.human
            || s.settling
            || s.agent.as_deref() != Some(agent)
            || !s.until.is_some_and(|until| until > Instant::now())
        {
            return Err(Error::Denied("lease is not owned or expired"));
        }
        s.until = Some(Instant::now() + lease);
        Ok(())
    }
    pub async fn expire(&self) -> bool {
        let mut s = self.inner.lock().await;
        if s.until.is_some_and(|until| until <= Instant::now()) {
            s.until = None;
            s.agent = None;
            s.generation += 1;
            return true;
        }
        false
    }
    pub async fn release(&self) {
        let mut s = self.inner.lock().await;
        s.human = false;
        s.agent = None;
        s.until = None;
        s.generation += 1;
    }
    pub async fn check(&self, agent: &str) -> Result<u64> {
        let s = self.inner.lock().await;
        if s.human
            || s.agent.as_deref() != Some(agent)
            || !s.until.is_some_and(|until| until > Instant::now())
        {
            return Err(Error::Denied("human takeover or expired lease"));
        }
        Ok(s.generation)
    }
    pub async fn acquire(&self, agent: &str, tab: &str) -> Result<Permit> {
        let generation = self.check(agent).await?;
        let lock = self
            .tabs
            .lock()
            .await
            .entry(tab.into())
            .or_default()
            .clone();
        let guard = lock.lock_owned().await;
        // A command waiting for a tab is never replayed after takeover/regrant.
        if self.check(agent).await? != generation {
            return Err(Error::Denied("stale queued command"));
        }
        Ok(Permit {
            generation,
            _tab: guard,
        })
    }
    /// Revocation is synchronous with this await. Caller then waits 500 ms and
    /// stops page loading before calling finish_takeover; scripts may keep running.
    pub async fn begin_takeover(&self) {
        let mut s = self.inner.lock().await;
        s.human = true;
        s.agent = None;
        s.until = None;
        s.settling = true;
        s.generation += 1;
    }
    pub async fn finish_takeover(&self) {
        self.inner.lock().await.settling = false;
    }
    pub async fn snapshot(&self) -> ControlState {
        let s = self.inner.lock().await;
        ControlState {
            owner: if s.human {
                "human"
            } else if s.until.is_some_and(|t| t > Instant::now()) {
                "agent"
            } else {
                "none"
            }
            .into(),
            agent_id: s.agent.clone(),
            lease_expires_at: None,
            settling: s.settling,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn queued_commands_are_not_replayed() {
        let a = Arbitration::default();
        a.grant("a".into(), Duration::from_secs(1)).await.unwrap();
        let permit = a.acquire("a", "tab").await.unwrap();
        let other = a.clone();
        let queued = tokio::spawn(async move { other.acquire("a", "tab").await });
        tokio::task::yield_now().await;
        a.begin_takeover().await;
        assert!(a.check("a").await.is_err());
        a.finish_takeover().await;
        a.release().await;
        a.grant("a".into(), Duration::from_secs(1)).await.unwrap();
        drop(permit);
        assert!(queued.await.unwrap().is_err());
    }
    #[tokio::test]
    async fn renewal_never_revives_expired_or_taken_over_lease() {
        let a = Arbitration::default();
        a.grant("a".into(), Duration::from_secs(1)).await.unwrap();
        let generation = a.check("a").await.unwrap();
        assert!(a.renew("b", Duration::from_secs(1)).await.is_err());
        a.renew("a", Duration::from_secs(2)).await.unwrap();
        assert_eq!(a.check("a").await.unwrap(), generation);
        a.begin_takeover().await;
        assert!(a.renew("a", Duration::from_secs(1)).await.is_err());
        a.finish_takeover().await;
        a.release().await;
        a.grant("a".into(), Duration::ZERO).await.unwrap();
        assert!(a.renew("a", Duration::from_secs(1)).await.is_err());
        assert!(a.expire().await);
        assert_eq!(a.snapshot().await.agent_id, None);
        assert!(!a.expire().await);
    }
    #[tokio::test]
    async fn lease_and_explicit_release() {
        let a = Arbitration::default();
        a.grant("a".into(), Duration::ZERO).await.unwrap();
        assert!(a.check("a").await.is_err());
        a.begin_takeover().await;
        assert!(a.grant("a".into(), Duration::from_secs(1)).await.is_err());
        a.finish_takeover().await;
        a.release().await;
        a.grant("a".into(), Duration::from_secs(1)).await.unwrap();
    }
}
