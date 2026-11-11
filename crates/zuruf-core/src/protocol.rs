use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const PROTOCOL_VERSION: u32 = 1;
pub type Id = String;
pub type Timestamp = String;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub repository_path: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t3_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile_id: Option<Id>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceStrategy {
    #[serde(rename = "new_worktree")]
    NewWorktree,
    #[serde(rename = "existing_worktree")]
    ExistingWorktree,
    #[serde(rename = "shared_root")]
    SharedRoot,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: Id,
    pub name: String,
    pub provider_instance_id: String,
    pub model: String,
    pub options: BTreeMap<String, StringOrBool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_mode: Option<String>,
    pub workspace_strategy: WorkspaceStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Catalog {
    pub fetched_at: Timestamp,
    pub providers: Vec<T3Provider>,
    pub runtime_modes: Vec<String>,
    pub interaction_modes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Provider {
    pub instance_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub constraints: Vec<String>,
    pub models: Vec<T3Model>,
    pub omitted_models: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3ModelOptionsItem {
    pub id: String,
    pub values: Vec<StringOrBool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<StringOrBool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Model {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub options: Vec<T3ModelOptionsItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3ProjectRef {
    pub id: String,
    pub title: String,
    pub workspace_root: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum T3ConnectionState {
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(rename = "connecting")]
    Connecting,
    #[serde(rename = "connected")]
    Connected,
    #[serde(rename = "error")]
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum T3ConnectionTransport {
    #[serde(rename = "mcp")]
    Mcp,
    #[serde(rename = "cli")]
    Cli,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3ConnectionCapabilities {
    pub model_options: bool,
    pub existing_worktree: bool,
    pub idempotent_launch: bool,
    pub interrupt: bool,
    pub attachments: bool,
    pub push: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3Connection {
    pub state: T3ConnectionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<T3ConnectionTransport>,
    pub capabilities: T3ConnectionCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMode {
    #[serde(rename = "shared")]
    Shared,
    #[serde(rename = "isolated")]
    Isolated,
    #[serde(rename = "observer")]
    Observer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlStateOwner {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "human")]
    Human,
    #[serde(rename = "agent")]
    Agent,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlState {
    pub owner: ControlStateOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<Timestamp>,
    pub settling: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: Id,
    pub session_id: Id,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub scriptable: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserSessionState {
    #[serde(rename = "stopped")]
    Stopped,
    #[serde(rename = "starting")]
    Starting,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "crashed")]
    Crashed,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSession {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub mode: SessionMode,
    pub state: BrowserSessionState,
    pub agent_access: bool,
    pub control: ControlState,
    pub tabs: Vec<BrowserTab>,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchState {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "sealed")]
    Sealed,
    #[serde(rename = "discarded")]
    Discarded,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub id: Id,
    pub review_id: Id,
    pub project_id: Id,
    pub seq: f64,
    pub state: BatchState,
    pub title: String,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Id>,
    pub created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed_at: Option<Timestamp>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationType {
    #[serde(rename = "viewport")]
    Viewport,
    #[serde(rename = "region")]
    Region,
    #[serde(rename = "dom")]
    Dom,
    #[serde(rename = "text")]
    Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    #[serde(rename = "screenshot")]
    Screenshot,
    #[serde(rename = "screenshot_marked")]
    ScreenshotMarked,
    #[serde(rename = "crop")]
    Crop,
    #[serde(rename = "audio")]
    Audio,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: Id,
    pub kind: AssetKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    pub url: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocatorCandidateKind {
    #[serde(rename = "testid")]
    Testid,
    #[serde(rename = "role")]
    Role,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "css")]
    Css,
    #[serde(rename = "xpath")]
    Xpath,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocatorCandidate {
    pub kind: LocatorCandidateKind,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playwright: Option<String>,
    pub stable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
/// Something the page reported on its own before a capture: a console message, an uncaught
/// exception or a request that failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub level: DiagnosticLevel,
    pub text: String,
    /// The script it came from, or the address that failed to load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// How often the same report came in a row.
    pub count: u32,
    /// How long before the capture it was last reported.
    pub age_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticKind {
    #[serde(rename = "console")]
    Console,
    #[serde(rename = "exception")]
    Exception,
    #[serde(rename = "network")]
    Network,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "warning")]
    Warning,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomCapture {
    pub locators: Vec<LocatorCandidate>,
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub bounding_box: Rect,
    pub frame_path: Vec<String>,
    pub shadow_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub children: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationViewport {
    pub width: f64,
    pub height: f64,
    pub device_scale_factor: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationScroll {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationCursor {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationTranscriptState {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "recording")]
    Recording,
    #[serde(rename = "transcribing")]
    Transcribing,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "failed")]
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationPriority {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "high")]
    High,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "wont_fix")]
    WontFix,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationGit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: Id,
    pub batch_id: Id,
    pub review_id: Id,
    pub project_id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<Id>,
    pub r#type: AnnotationType,
    pub order: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<AnnotationViewport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll: Option<AnnotationScroll>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<AnnotationCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomCapture>,
    /// Console errors, uncaught exceptions and failed requests of the page at capture time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    pub assets: Vec<Asset>,
    pub transcript: String,
    pub transcript_state: AnnotationTranscriptState,
    pub segment_ids: Vec<Id>,
    pub note: String,
    pub priority: AnnotationPriority,
    pub tags: Vec<String>,
    pub status: AnnotationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<AnnotationGit>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptSegmentState {
    #[serde(rename = "recording")]
    Recording,
    #[serde(rename = "transcribing")]
    Transcribing,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "empty")]
    Empty,
    #[serde(rename = "failed")]
    Failed,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: Id,
    pub batch_id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation_id: Option<Id>,
    pub started_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Timestamp>,
    pub state: TranscriptSegmentState,
    pub original_text: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    #[serde(rename = "finalizing_transcript")]
    FinalizingTranscript,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "dispatching")]
    Dispatching,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "needs_review")]
    NeedsReview,
    #[serde(rename = "needs_approval")]
    NeedsApproval,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "unknown_remote_state")]
    UnknownRemoteState,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: Id,
    pub batch_id: Id,
    pub project_id: Id,
    pub title: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_status: Option<String>,
    pub profile: AgentProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_request: Option<String>,
    pub workspace_conflict: bool,
    pub marked_done: bool,
    pub annotation_count: f64,
    pub created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<Timestamp>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskEventKind {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "message")]
    Message,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "follow_up")]
    FollowUp,
    #[serde(rename = "retry")]
    Retry,
    #[serde(rename = "note")]
    Note,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub id: Id,
    pub task_id: Id,
    pub at: Timestamp,
    pub kind: TaskEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeechStateState {
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "recording")]
    Recording,
    #[serde(rename = "transcribing")]
    Transcribing,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "busy_external")]
    BusyExternal,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechState {
    pub state: SpeechStateState,
    pub pending_segments: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_gap_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionId {
    #[serde(rename = "review.toggle")]
    ReviewToggle,
    #[serde(rename = "capture.viewport")]
    CaptureViewport,
    #[serde(rename = "capture.region")]
    CaptureRegion,
    #[serde(rename = "capture.dom")]
    CaptureDom,
    #[serde(rename = "comment.text")]
    CommentText,
    #[serde(rename = "speech.toggle")]
    SpeechToggle,
    #[serde(rename = "batch.dispatch")]
    BatchDispatch,
    #[serde(rename = "batch.send_to_thread")]
    BatchSendToThread,
    #[serde(rename = "control.takeover")]
    ControlTakeover,
    #[serde(rename = "action.cancel")]
    ActionCancel,
    #[serde(rename = "panel.toggle")]
    PanelToggle,
    #[serde(rename = "app.open")]
    AppOpen,
    #[serde(rename = "profile.slot1")]
    ProfileSlot1,
    #[serde(rename = "profile.slot2")]
    ProfileSlot2,
    #[serde(rename = "profile.slot3")]
    ProfileSlot3,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modifier {
    #[serde(rename = "ctrl")]
    Ctrl,
    #[serde(rename = "alt")]
    Alt,
    #[serde(rename = "shift")]
    Shift,
    #[serde(rename = "super")]
    Super,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    pub mods: Vec<Modifier>,
    pub code: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingScope {
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "global")]
    Global,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub action: ActionId,
    pub scope: BindingScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord: Option<Chord>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingIssueScope {
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "global")]
    Global,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingIssueKind {
    #[serde(rename = "duplicate")]
    Duplicate,
    #[serde(rename = "needs_modifier")]
    NeedsModifier,
    #[serde(rename = "taken_by_system")]
    TakenBySystem,
    #[serde(rename = "unsupported_key")]
    UnsupportedKey,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingIssue {
    pub action: ActionId,
    pub scope: BindingIssueScope,
    pub kind: BindingIssueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsCaptureMode {
    #[serde(rename = "new_entry")]
    NewEntry,
    #[serde(rename = "extend_open_entry")]
    ExtendOpenEntry,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsCapture {
    pub ring_color: String,
    pub ring_radius: f64,
    pub keep_raw_screenshot: bool,
    pub mode: SettingsCaptureMode,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsSpeechBackend {
    #[serde(rename = "continuous")]
    Continuous,
    #[serde(rename = "daemon")]
    Daemon,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsSpeechAttach {
    #[serde(rename = "after")]
    After,
    #[serde(rename = "before")]
    Before,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSpeech {
    pub enabled: bool,
    pub backend: SettingsSpeechBackend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub attach: SettingsSpeechAttach,
    pub keep_audio: bool,
    pub voxtype_path: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRedaction {
    pub strip_query: bool,
    pub patterns: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsT3 {
    pub cli_path: String,
    pub poll_seconds: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsUiTheme {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "light")]
    Light,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsUiLanguage {
    #[serde(rename = "de")]
    De,
    #[serde(rename = "en")]
    En,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUi {
    pub theme: SettingsUiTheme,
    pub language: SettingsUiLanguage,
    pub open_panel_on_review_start: bool,
    #[serde(default)]
    pub browser: SettingsUiBrowser,
}
/// Where the session browser lives: next to the app in one window, or in a window of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsUiBrowser {
    #[serde(rename = "separate")]
    Separate,
    /// The default: one window carries the app and the pages under review.
    #[default]
    #[serde(rename = "integrated")]
    Integrated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub bindings: Vec<Binding>,
    pub capture: SettingsCapture,
    pub speech: SettingsSpeech,
    pub redaction: SettingsRedaction,
    pub t3: SettingsT3,
    pub ui: SettingsUi,
    pub data_dir: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub protocol_version: f64,
    pub daemon_version: String,
    pub review_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_project_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_review_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_batch_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<ActionId>,
    pub speech: SpeechState,
    pub t3: T3Connection,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: AppStatus,
    pub projects: Vec<Project>,
    pub profiles: Vec<AgentProfile>,
    pub sessions: Vec<BrowserSession>,
    pub reviews: Vec<Review>,
    pub batches: Vec<Batch>,
    pub annotations: Vec<Annotation>,
    pub segments: Vec<TranscriptSegment>,
    pub tasks: Vec<Task>,
    pub settings: Settings,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeLevel {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub level: NoticeLevel,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_id: Option<Id>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<Vec<BindingIssue>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub error: ApiErrorError,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up_task_id: Option<Id>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchResponse {
    pub sealed_batch: Batch,
    pub task: Task,
    pub active_batch: Batch,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUpRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation_ids: Option<Vec<Id>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolveRequestResolution {
    #[serde(rename = "adopt_thread")]
    AdoptThread,
    #[serde(rename = "not_sent")]
    NotSent,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRequest {
    pub resolution: ResolveRequestResolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlRequestAction {
    #[serde(rename = "takeover")]
    Takeover,
    #[serde(rename = "release")]
    Release,
    #[serde(rename = "grant")]
    Grant,
    #[serde(rename = "renew")]
    Renew,
    #[serde(rename = "revoke")]
    Revoke,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlRequest {
    pub action: ControlRequestAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_seconds: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerEvent {
    #[serde(rename = "app.open_requested")]
    AppOpenRequested(AppOpenRequested),
    #[serde(rename = "snapshot")]
    Snapshot(Snapshot),
    #[serde(rename = "status.changed")]
    StatusChanged(AppStatus),
    #[serde(rename = "speech.level")]
    SpeechLevel(SpeechLevel),
    #[serde(rename = "speech.partial")]
    SpeechPartial(SpeechPartial),
    #[serde(rename = "project.upserted")]
    ProjectUpserted(Project),
    #[serde(rename = "profile.upserted")]
    ProfileUpserted(AgentProfile),
    #[serde(rename = "session.upserted")]
    SessionUpserted(BrowserSession),
    #[serde(rename = "review.upserted")]
    ReviewUpserted(Review),
    #[serde(rename = "batch.upserted")]
    BatchUpserted(Batch),
    #[serde(rename = "annotation.upserted")]
    AnnotationUpserted(Annotation),
    #[serde(rename = "segment.upserted")]
    SegmentUpserted(TranscriptSegment),
    #[serde(rename = "task.upserted")]
    TaskUpserted(Task),
    #[serde(rename = "project.removed")]
    ProjectRemoved(Removed),
    #[serde(rename = "profile.removed")]
    ProfileRemoved(Removed),
    #[serde(rename = "session.removed")]
    SessionRemoved(Removed),
    #[serde(rename = "annotation.removed")]
    AnnotationRemoved(AnnotationRemoved),
    #[serde(rename = "task.event")]
    TaskEvent(TaskEvent),
    #[serde(rename = "settings.changed")]
    SettingsChanged(Settings),
    #[serde(rename = "notice")]
    Notice(Notice),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub seq: u64,
    #[serde(flatten)]
    pub event: ServerEvent,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Removed {
    pub id: Id,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationRemoved {
    pub id: Id,
    pub batch_id: Id,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechLevel {
    pub level: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechPartial {
    pub segment_id: Id,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrBool {
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppOpenRequested {
    pub view: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3PairRequest {
    pub origin: Option<String>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub name: String,
    pub repository_path: String,
    pub base_url: String,
    pub t3_project_id: Option<String>,
}
