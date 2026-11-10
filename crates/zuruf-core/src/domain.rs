use crate::protocol::*;
use serde_json::{Value, json};
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("illegal state transition")]
    Transition,
    #[error("manifest path must be absolute and asset paths relative")]
    Path,
}
impl BatchState {
    pub fn transition(self, next: Self) -> Result<Self, DomainError> {
        if self == Self::Draft && matches!(next, Self::Sealed | Self::Discarded) {
            Ok(next)
        } else {
            Err(DomainError::Transition)
        }
    }
}
impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
    pub fn transition(self, next: Self) -> Result<Self, DomainError> {
        use TaskStatus::*;
        let legal = match self {
            FinalizingTranscript => matches!(next, Queued | Failed | Cancelled),
            Queued => matches!(next, Dispatching | Cancelled | Failed),
            Dispatching => matches!(
                next,
                Queued
                    | Running
                    | NeedsApproval
                    | NeedsReview
                    | Completed
                    | Failed
                    | Cancelled
                    | UnknownRemoteState
            ),
            Running | NeedsApproval | NeedsReview => matches!(
                next,
                Queued
                    | Running
                    | NeedsApproval
                    | NeedsReview
                    | Completed
                    | Failed
                    | Cancelled
                    | UnknownRemoteState
            ),
            Failed | Cancelled | Completed => matches!(next, Queued | Running | UnknownRemoteState),
            UnknownRemoteState => matches!(next, Queued | Running | Cancelled),
        };
        if legal && self != next {
            Ok(next)
        } else {
            Err(DomainError::Transition)
        }
    }
}
pub fn map_remote_status(remote: &str, approval: bool, pending: bool) -> TaskStatus {
    use TaskStatus::*;
    if matches!(remote, "failed" | "interrupted" | "cancelled") {
        return if remote == "failed" {
            Failed
        } else {
            Cancelled
        };
    }
    if pending {
        return if approval { NeedsApproval } else { NeedsReview };
    }
    match remote {
        "preparing" | "queued" => Queued,
        "starting" | "running" => Running,
        "waiting" if approval => NeedsApproval,
        "waiting" | "rolled_back" | "idle" => NeedsReview,
        "completed" => Completed,
        "failed" => Failed,
        "cancelled" | "interrupted" => Cancelled,
        _ => UnknownRemoteState,
    }
}
impl ControlState {
    pub fn take_over(&mut self, in_flight: bool) {
        self.owner = ControlStateOwner::Human;
        self.agent_id = None;
        self.lease_expires_at = None;
        self.settling = in_flight;
    }
    pub fn release(&mut self) {
        self.owner = ControlStateOwner::None;
        self.agent_id = None;
        self.lease_expires_at = None;
        self.settling = false;
    }
    pub fn grant(&mut self, agent: String, expires: Timestamp) -> Result<(), DomainError> {
        if self.owner != ControlStateOwner::None || self.settling {
            return Err(DomainError::Transition);
        }
        self.owner = ControlStateOwner::Agent;
        self.agent_id = Some(agent);
        self.lease_expires_at = Some(expires);
        Ok(())
    }
    pub fn settle(&mut self) {
        self.settling = false;
    }
    /// Invalid timestamps never grant control implicitly.
    pub fn expire(&mut self, now: &str) {
        let parse = |t: &str| {
            time::OffsetDateTime::parse(t, &time::format_description::well_known::Rfc3339)
        };
        if self.owner == ControlStateOwner::Agent
            && self
                .lease_expires_at
                .as_deref()
                .is_some_and(|t| match (parse(t), parse(now)) {
                    (Ok(expiry), Ok(now)) => expiry <= now,
                    _ => true,
                })
        {
            self.release();
        }
    }
}
pub fn validate_bindings(bindings: &[Binding]) -> Vec<BindingIssue> {
    let mut issues = Vec::new();
    for (index, b) in bindings.iter().enumerate() {
        let Some(c) = &b.chord else { continue };
        let scope = match b.scope {
            BindingScope::Page => BindingIssueScope::Page,
            BindingScope::Global => BindingIssueScope::Global,
        };
        let mut add = |kind| {
            issues.push(BindingIssue {
                action: b.action,
                scope,
                kind,
                detail: None,
            })
        };
        if b.scope == BindingScope::Global && c.mods.is_empty() {
            add(BindingIssueKind::NeedsModifier);
        }
        let supported = (c.code.starts_with("Key")
            && c.code.len() == 4
            && c.code.as_bytes()[3].is_ascii_uppercase())
            || (c.code.starts_with("Digit")
                && c.code.len() == 6
                && c.code.as_bytes()[5].is_ascii_digit())
            || matches!(
                c.code.as_str(),
                "Escape"
                    | "Period"
                    | "Space"
                    | "Enter"
                    | "Tab"
                    | "Backspace"
                    | "ArrowUp"
                    | "ArrowDown"
                    | "ArrowLeft"
                    | "ArrowRight"
            )
            || (c
                .code
                .strip_prefix('F')
                .and_then(|v| v.parse::<u8>().ok())
                .is_some_and(|n| (1..=24).contains(&n)));
        if !supported {
            add(BindingIssueKind::UnsupportedKey);
        }
        let mut mods = c.mods.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>();
        mods.sort();
        if mods.windows(2).any(|p| p[0] == p[1]) {
            add(BindingIssueKind::UnsupportedKey);
        }
        if bindings[..index].iter().any(|other| {
            other.scope == b.scope
                && other.chord.as_ref().is_some_and(|x| {
                    x.code == c.code && {
                        let mut m = x.mods.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>();
                        m.sort();
                        m == mods
                    }
                })
        }) {
            add(BindingIssueKind::Duplicate);
        }
    }
    issues
}
pub fn redact_url(raw: &str, settings: &SettingsRedaction) -> String {
    let Ok(mut u) = url::Url::parse(raw) else {
        return String::new();
    };
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_fragment(None);
    if settings.strip_query {
        u.set_query(None);
    } else {
        let pairs = u
            .query_pairs()
            .filter(|(k, _)| !sensitive(k, &settings.patterns))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        u.set_query(None);
        if !pairs.is_empty() {
            u.query_pairs_mut().extend_pairs(pairs);
        }
    }
    u.to_string()
}
pub fn sensitive(key: &str, patterns: &[String]) -> bool {
    let k = key.to_lowercase();
    [
        "token",
        "cookie",
        "secret",
        "password",
        "authorization",
        "session",
        "credential",
    ]
    .iter()
    .any(|p| k.contains(p))
        || patterns.iter().any(|p| k.contains(&p.to_lowercase()))
}
pub fn build_manifest(
    batch: &Batch,
    project: &Project,
    profile: &AgentProfile,
    annotations: &[Annotation],
    settings: &SettingsRedaction,
) -> Result<Value, DomainError> {
    let mut entries = Vec::new();
    for a in annotations {
        if a.assets.iter().any(|a| {
            a.path.starts_with('/')
                || a.path.split('/').any(|p| p == ".." || p.is_empty())
                || a.path.contains('\\')
        }) {
            return Err(DomainError::Path);
        }
        let mut dom = a.dom.clone();
        if let Some(d) = &mut dom {
            d.attributes.retain(|k, _| {
                matches!(
                    k.as_str(),
                    "id" | "class"
                        | "role"
                        | "data-testid"
                        | "aria-label"
                        | "aria-labelledby"
                        | "type"
                ) && !sensitive(k, &settings.patterns)
            });
        }
        entries.push(json!({"id":a.id,"type":a.r#type,"transcript":a.transcript,"note":a.note,"captureTime":a.created_at,"url":a.url.as_deref().map(|u|redact_url(u,settings)),"screenshot":a.assets.first().map(|a|&a.path),"assets":a.assets.iter().map(|a|&a.path).collect::<Vec<_>>(),"dom":dom,"boundingBox":a.rect,"sessionId":a.session_id,"tabId":a.tab_id,"viewport":a.viewport}));
    }
    Ok(
        json!({"schemaVersion":"1.0","batchId":batch.id,"note":batch.note,"project":{"name":project.name,"repositoryPath":project.repository_path,"baseUrl":redact_url(&project.base_url,settings)},"browserContext":annotations.first().map(|a| json!({"sessionId":a.session_id,"tabId":a.tab_id,"url":a.url.as_deref().map(|u|redact_url(u,settings)),"title":a.page_title,"viewport":a.viewport})),"annotations":entries,"task":{"goal":"Implementiere die annotierten Änderungen.","workspaceStrategy":profile.workspace_strategy,"verifyInBrowser":true}}),
    )
}
pub fn build_prompt(absolute_manifest_path: &str) -> Result<String, DomainError> {
    if !absolute_manifest_path.starts_with('/') {
        return Err(DomainError::Path);
    }
    Ok(format!(
        "Analysiere und bearbeite die Review-Annotationen. Das vollständige Manifest liegt unter {absolute_manifest_path}; Screenshots sind relativ zu diesem Verzeichnis referenziert. Prüfe die DOM-Kontexte und implementiere die dokumentierten Änderungen im zugewiesenen Projekt-Workspace. Füge keine nicht dokumentierten Anforderungen hinzu. Prüfe die Änderungen im Browser, falls eine passende Session freigegeben ist. Melde Ergebnis, betroffene Dateien und offene Punkte je Annotation zurück."
    ))
}
impl Default for Settings {
    fn default() -> Self {
        let mut bindings = vec![];
        for (action, code, mods, scope) in [
            (
                ActionId::CaptureViewport,
                "KeyW",
                vec![],
                BindingScope::Page,
            ),
            (ActionId::CaptureRegion, "KeyS", vec![], BindingScope::Page),
            (ActionId::CaptureDom, "KeyD", vec![], BindingScope::Page),
            (ActionId::CommentText, "KeyE", vec![], BindingScope::Page),
            (ActionId::SpeechToggle, "KeyR", vec![], BindingScope::Page),
            (
                ActionId::BatchDispatch,
                "KeyN",
                vec![Modifier::Shift],
                BindingScope::Page,
            ),
            (ActionId::ActionCancel, "Escape", vec![], BindingScope::Page),
            (
                ActionId::ReviewToggle,
                "KeyR",
                vec![Modifier::Super, Modifier::Alt],
                BindingScope::Global,
            ),
            (
                ActionId::ControlTakeover,
                "Period",
                vec![Modifier::Super, Modifier::Alt],
                BindingScope::Global,
            ),
            (
                ActionId::AppOpen,
                "KeyZ",
                vec![Modifier::Super, Modifier::Alt],
                BindingScope::Global,
            ),
        ] {
            bindings.push(Binding {
                action,
                scope,
                chord: Some(Chord {
                    code: code.into(),
                    mods,
                }),
            });
        }
        for (action, code, mods) in [
            (ActionId::PanelToggle, "KeyP", vec![]),
            (ActionId::ProfileSlot1, "Digit1", vec![Modifier::Shift]),
            (ActionId::ProfileSlot2, "Digit2", vec![Modifier::Shift]),
            (ActionId::ProfileSlot3, "Digit3", vec![Modifier::Shift]),
        ] {
            bindings.push(Binding {
                action,
                scope: BindingScope::Page,
                chord: Some(Chord {
                    code: code.into(),
                    mods,
                }),
            });
        }
        bindings.push(Binding {
            action: ActionId::BatchSendToThread,
            scope: BindingScope::Page,
            chord: None,
        });
        for action in [ActionId::BatchDispatch, ActionId::SpeechToggle] {
            bindings.push(Binding {
                action,
                scope: BindingScope::Global,
                chord: None,
            });
        }
        // Keep ordering identical to the UI's defaultBindings(), including unbound actions.
        bindings.sort_by_key(|binding| match (binding.scope, binding.action) {
            (BindingScope::Page, ActionId::CaptureViewport) => 0,
            (BindingScope::Page, ActionId::CaptureRegion) => 1,
            (BindingScope::Page, ActionId::CaptureDom) => 2,
            (BindingScope::Page, ActionId::CommentText) => 3,
            (BindingScope::Page, ActionId::SpeechToggle) => 4,
            (BindingScope::Page, ActionId::BatchDispatch) => 5,
            (BindingScope::Page, ActionId::ActionCancel) => 6,
            (BindingScope::Page, ActionId::PanelToggle) => 7,
            (BindingScope::Page, ActionId::BatchSendToThread) => 8,
            (BindingScope::Page, ActionId::ProfileSlot1) => 9,
            (BindingScope::Page, ActionId::ProfileSlot2) => 10,
            (BindingScope::Page, ActionId::ProfileSlot3) => 11,
            (BindingScope::Global, ActionId::ReviewToggle) => 12,
            (BindingScope::Global, ActionId::ControlTakeover) => 13,
            (BindingScope::Global, ActionId::AppOpen) => 14,
            (BindingScope::Global, ActionId::BatchDispatch) => 15,
            _ => 16,
        });
        Self {
            bindings,
            capture: SettingsCapture {
                ring_color: "#ff4d4d".into(),
                ring_radius: 24.,
                keep_raw_screenshot: true,
                mode: SettingsCaptureMode::NewEntry,
            },
            speech: SettingsSpeech {
                enabled: false,
                backend: SettingsSpeechBackend::Continuous,
                model: None,
                language: Some("de".into()),
                attach: SettingsSpeechAttach::After,
                keep_audio: false,
                voxtype_path: "voxtype".into(),
            },
            redaction: SettingsRedaction {
                strip_query: true,
                patterns: vec![],
            },
            t3: SettingsT3 {
                cli_path: "t3".into(),
                poll_seconds: 2.,
            },
            ui: SettingsUi {
                theme: SettingsUiTheme::System,
                language: SettingsUiLanguage::De,
                open_panel_on_review_start: true,
            },
            data_dir: String::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_exhaustive() {
        for a in [BatchState::Draft, BatchState::Sealed, BatchState::Discarded] {
            for b in [BatchState::Draft, BatchState::Sealed, BatchState::Discarded] {
                assert_eq!(
                    a.transition(b).is_ok(),
                    a == BatchState::Draft && b != BatchState::Draft
                );
            }
        }
    }
    #[test]
    fn tasks_exhaustive() {
        use TaskStatus::*;
        let all = [
            FinalizingTranscript,
            Queued,
            Dispatching,
            Running,
            Completed,
            NeedsReview,
            NeedsApproval,
            Failed,
            Cancelled,
            UnknownRemoteState,
        ];
        let expected = [
            vec![Queued, Failed, Cancelled],
            vec![Dispatching, Cancelled, Failed],
            vec![
                Queued,
                Running,
                NeedsApproval,
                NeedsReview,
                Completed,
                Failed,
                Cancelled,
                UnknownRemoteState,
            ],
            vec![
                Queued,
                NeedsApproval,
                NeedsReview,
                Completed,
                Failed,
                Cancelled,
                UnknownRemoteState,
            ],
            vec![Queued, Running, UnknownRemoteState],
            vec![
                Queued,
                Running,
                NeedsApproval,
                Completed,
                Failed,
                Cancelled,
                UnknownRemoteState,
            ],
            vec![
                Queued,
                Running,
                NeedsReview,
                Completed,
                Failed,
                Cancelled,
                UnknownRemoteState,
            ],
            vec![Queued, Running, UnknownRemoteState],
            vec![Queued, Running, UnknownRemoteState],
            vec![Queued, Running, Cancelled],
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all {
                assert_eq!(
                    a.transition(b).is_ok(),
                    expected[i].contains(&b),
                    "{a:?}->{b:?}"
                );
            }
        }
    }
    #[test]
    fn control_lease_and_takeover() {
        let mut c = ControlState {
            owner: ControlStateOwner::None,
            agent_id: None,
            lease_expires_at: None,
            settling: false,
        };
        c.grant("a".into(), "2026-11-11T00:00:00Z".into()).unwrap();
        c.take_over(true);
        assert!(c.grant("b".into(), "x".into()).is_err());
        c.settle();
        c.release();
        c.grant("b".into(), "2026-11-11T00:00:00Z".into()).unwrap();
        c.expire("2026-11-12T00:00:00Z");
        assert_eq!(c.owner, ControlStateOwner::None);
    }
    #[test]
    fn bindings() {
        let mut b = Settings::default().bindings;
        assert!(validate_bindings(&b).is_empty());
        b.push(b[0].clone());
        assert_eq!(validate_bindings(&b)[0].kind, BindingIssueKind::Duplicate);
        b[0].scope = BindingScope::Global;
        b[0].chord.as_mut().unwrap().code = "bad".into();
        let i = validate_bindings(&b);
        assert!(i.iter().any(|i| i.kind == BindingIssueKind::NeedsModifier));
        assert!(i.iter().any(|i| i.kind == BindingIssueKind::UnsupportedKey));
    }
    #[test]
    fn redaction() {
        assert_eq!(
            redact_url(
                "https://user:pw@host/path?token=bad&ok=1#secret",
                &SettingsRedaction {
                    strip_query: false,
                    patterns: vec![]
                }
            ),
            "https://host/path?ok=1"
        );
        assert!(redact_url("invalid", &Settings::default().redaction).is_empty());
    }
    #[test]
    fn statuses() {
        assert_eq!(
            map_remote_status("waiting", false, false),
            TaskStatus::NeedsReview
        );
        assert_eq!(
            map_remote_status("waiting", true, true),
            TaskStatus::NeedsApproval
        );
        assert_eq!(
            map_remote_status("novel", false, false),
            TaskStatus::UnknownRemoteState
        );
    }
    #[test]
    fn prompt() {
        assert!(build_prompt("relative").is_err());
        assert!(
            build_prompt("/safe/batch.json")
                .unwrap()
                .contains("je Annotation")
        );
    }
}
/// Monotonic control arbitration; callers supply time, so this remains pure.
#[derive(Debug, Clone)]
pub struct ControlArbiter {
    pub state: ControlState,
    lease_deadline_ms: Option<u64>,
    settle_deadline_ms: Option<u64>,
}
impl Default for ControlArbiter {
    fn default() -> Self {
        Self {
            state: ControlState {
                owner: ControlStateOwner::None,
                agent_id: None,
                lease_expires_at: None,
                settling: false,
            },
            lease_deadline_ms: None,
            settle_deadline_ms: None,
        }
    }
}
impl ControlArbiter {
    pub fn grant(
        &mut self,
        agent: String,
        expires: Timestamp,
        now_ms: u64,
        lease_ms: u64,
    ) -> Result<(), DomainError> {
        if agent.is_empty() || lease_ms == 0 {
            return Err(DomainError::Transition);
        }
        self.state.grant(agent, expires)?;
        self.lease_deadline_ms = Some(now_ms.saturating_add(lease_ms));
        Ok(())
    }
    pub fn takeover(&mut self, now_ms: u64, in_flight: bool) {
        self.state.take_over(in_flight);
        self.lease_deadline_ms = None;
        self.settle_deadline_ms = in_flight.then(|| now_ms.saturating_add(500));
    }
    pub fn release(&mut self) -> Result<(), DomainError> {
        if self.state.settling {
            return Err(DomainError::Transition);
        }
        self.state.release();
        self.lease_deadline_ms = None;
        Ok(())
    }
    /// True means the browser must stop loading after the settling budget elapsed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        if self
            .lease_deadline_ms
            .is_some_and(|deadline| now_ms >= deadline)
        {
            self.state.release();
            self.lease_deadline_ms = None;
        }
        if self
            .settle_deadline_ms
            .is_some_and(|deadline| now_ms >= deadline)
        {
            self.state.settle();
            self.settle_deadline_ms = None;
            return true;
        }
        false
    }
    pub fn command_finished(&mut self) {
        self.state.settle();
        self.settle_deadline_ms = None;
    }
    pub fn accepts(&self, agent: &str, now_ms: u64) -> bool {
        self.state.owner == ControlStateOwner::Agent
            && self.state.agent_id.as_deref() == Some(agent)
            && self
                .lease_deadline_ms
                .is_some_and(|deadline| now_ms < deadline)
            && !self.state.settling
    }
}
#[cfg(test)]
mod arbitration_tests {
    use super::*;
    #[test]
    fn ownership_transitions_exhaustive() {
        for owner in [
            ControlStateOwner::None,
            ControlStateOwner::Human,
            ControlStateOwner::Agent,
        ] {
            for settling in [false, true] {
                let mut c = ControlState {
                    owner,
                    agent_id: None,
                    lease_expires_at: None,
                    settling,
                };
                assert_eq!(
                    c.grant("a".into(), "2027-01-01T00:00:00Z".into()).is_ok(),
                    owner == ControlStateOwner::None && !settling
                );
                c.take_over(settling);
                assert_eq!(c.owner, ControlStateOwner::Human);
                assert_eq!(c.settling, settling);
                c.release();
                assert_eq!(c.owner, ControlStateOwner::None);
                assert!(!c.settling);
            }
        }
    }
    #[test]
    fn settling_and_lease_budget() {
        let mut c = ControlArbiter::default();
        c.grant("a".into(), "2027-01-01T00:00:00Z".into(), 0, 1000)
            .unwrap();
        assert!(c.accepts("a", 0));
        assert!(!c.accepts("b", 0));
        c.takeover(100, true);
        assert!(!c.accepts("a", 100));
        assert!(c.release().is_err());
        assert!(!c.tick(599));
        assert!(c.tick(600));
        c.release().unwrap();
        c.grant("a".into(), "2027-01-01T00:00:00Z".into(), 600, 100)
            .unwrap();
        assert!(!c.accepts("a", 700));
        assert!(!c.tick(700));
        assert_eq!(c.state.owner, ControlStateOwner::None);
    }
    #[test]
    fn bindings_scopes_and_modifier_order() {
        let mut bindings = Settings::default().bindings;
        let mut b = bindings[5].clone();
        b.scope = BindingScope::Global;
        bindings.push(b.clone());
        assert!(validate_bindings(&bindings).is_empty());
        b.chord.as_mut().unwrap().mods = vec![Modifier::Alt, Modifier::Shift];
        bindings.last_mut().unwrap().chord = b.chord.clone();
        b.chord.as_mut().unwrap().mods.reverse();
        bindings.push(b);
        assert_eq!(validate_bindings(&bindings).len(), 1);
    }
    #[test]
    fn every_remote_status() {
        for (remote, expected) in [
            ("preparing", TaskStatus::Queued),
            ("queued", TaskStatus::Queued),
            ("starting", TaskStatus::Running),
            ("running", TaskStatus::Running),
            ("waiting", TaskStatus::NeedsReview),
            ("completed", TaskStatus::Completed),
            ("failed", TaskStatus::Failed),
            ("cancelled", TaskStatus::Cancelled),
            ("interrupted", TaskStatus::Cancelled),
            ("rolled_back", TaskStatus::NeedsReview),
            ("idle", TaskStatus::NeedsReview),
            ("timeout", TaskStatus::UnknownRemoteState),
        ] {
            assert_eq!(map_remote_status(remote, false, false), expected);
        }
    }
}
