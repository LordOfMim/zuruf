//! Object-safe integration boundary. No runtime or IO implementation lives here.
use crate::protocol::*;
use std::{future::Future, pin::Pin};
pub type AdapterFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;
#[derive(Debug, Clone, thiserror::Error)]
pub enum AdapterError {
    #[error("adapter unavailable: {0}")]
    Unavailable(String),
    #[error("request outcome is ambiguous: {0}")]
    Ambiguous(String),
    #[error("request rejected: {0}")]
    Rejected(String),
    #[error("operation unsupported: {0}")]
    Unsupported(String),
    #[error("adapter IO: {0}")]
    Io(String),
}
#[derive(Debug, Clone)]
pub struct CreateThreadRequest {
    pub client_request_id: Id,
    pub profile: AgentProfile,
    pub project: Project,
    pub title: String,
    pub prompt: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReceipt {
    pub thread_id: String,
    pub thread_url: Option<String>,
    pub run_id: String,
    pub workspace_path: Option<String>,
    pub branch: Option<String>,
}
#[derive(Debug, Clone)]
pub struct RemoteStatus {
    pub run_id: String,
    pub status: String,
    pub last_assistant_message: Option<String>,
    pub pending_request: Option<String>,
    pub approval: bool,
    pub error: Option<String>,
}
pub trait T3Adapter: Send + Sync {
    fn pair(&self, _origin: Option<String>) -> AdapterFuture<'_, T3Connection> {
        self.discover_capabilities()
    }
    fn reconnect(&self) -> AdapterFuture<'_, T3Connection> {
        self.discover_capabilities()
    }
    fn discover_capabilities(&self) -> AdapterFuture<'_, T3Connection>;
    fn list_projects(&self) -> AdapterFuture<'_, Vec<T3ProjectRef>>;
    fn catalog(&self, search: Option<String>) -> AdapterFuture<'_, T3Catalog>;
    fn create_thread(&self, input: CreateThreadRequest) -> AdapterFuture<'_, ThreadReceipt>;
    /// Marked image bytes loaded from this batch's committed assets.
    fn create_thread_with_assets(
        &self,
        input: CreateThreadRequest,
        _assets: Vec<(String, Vec<u8>)>,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        self.create_thread(input)
    }
    fn send_message(
        &self,
        thread: String,
        client_request_id: String,
        prompt: String,
    ) -> AdapterFuture<'_, ThreadReceipt>;
    fn send_message_with_assets(
        &self,
        thread: String,
        client_request_id: String,
        prompt: String,
        _assets: Vec<(String, Vec<u8>)>,
    ) -> AdapterFuture<'_, ThreadReceipt> {
        self.send_message(thread, client_request_id, prompt)
    }
    /// An empty run is used only after explicit adopt_thread resolution: resolve
    /// the thread's active/latest run and return its actual ID. Otherwise return
    /// messages and pending requests strictly for the requested run.
    fn thread_status(&self, thread: String, run: String) -> AdapterFuture<'_, RemoteStatus>;
    fn find_threads_by_marker(
        &self,
        project: Project,
        marker: String,
    ) -> AdapterFuture<'_, Vec<ThreadReceipt>>;
    fn interrupt(&self, thread: String) -> AdapterFuture<'_, ()>;
}
#[derive(Debug, Clone)]
pub struct SpeechBoundary {
    pub batch_id: Id,
    pub annotation_id: Option<Id>,
    pub at: Timestamp,
}
/// Ownership is fixed when the segment is created, before its future resolves.
pub struct SegmentHandle {
    pub segment: TranscriptSegment,
    pub transcript: AdapterFuture<'static, String>,
}
pub trait SpeechAdapter: Send + Sync {
    fn state(&self) -> AdapterFuture<'_, SpeechState>;
    fn start_epoch(&self, boundary: SpeechBoundary) -> AdapterFuture<'_, ()>;
    fn mark(&self, boundary: SpeechBoundary) -> AdapterFuture<'_, Vec<SegmentHandle>>;
    /// The daemon runs stop/start in its serialized background boundary queue.
    /// Segment handles must retain original ownership even when finalization is slow.
    fn stop(&self) -> AdapterFuture<'_, Vec<SegmentHandle>>;
    fn cancel(&self) -> AdapterFuture<'_, ()>;
}
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub kind: AnnotationType,
    pub rect: Option<Rect>,
}
#[derive(Debug, Clone)]
pub struct CaptureImage {
    pub kind: AssetKind,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub images: Vec<CaptureImage>,
    pub tab: BrowserTab,
    pub viewport: Option<AnnotationViewport>,
    pub scroll: Option<AnnotationScroll>,
    pub cursor: Option<AnnotationCursor>,
    pub rect: Option<Rect>,
    pub dom: Option<DomCapture>,
    /// What the page reported before the capture, oldest first.
    pub diagnostics: Vec<Diagnostic>,
}
pub trait BrowserController: Send + Sync {
    fn restart_session(&self, source: BrowserSession) -> AdapterFuture<'_, BrowserSession> {
        Box::pin(async move {
            self.stop_session(source.id.clone()).await?;
            self.start_session(source).await
        })
    }
    fn start_session(&self, session: BrowserSession) -> AdapterFuture<'_, BrowserSession>;
    fn stop_session(&self, session: Id) -> AdapterFuture<'_, ()>;
    fn duplicate_session(
        &self,
        source: BrowserSession,
        new_id: Id,
    ) -> AdapterFuture<'_, BrowserSession>;
    fn open_tab(&self, session: Id, url: String) -> AdapterFuture<'_, BrowserTab>;
    fn list_tabs(&self, session: Id) -> AdapterFuture<'_, Vec<BrowserTab>>;
    fn capture(&self, request: CaptureRequest) -> AdapterFuture<'_, CaptureResult>;
    fn confirm_capture(&self, _session: Id) -> AdapterFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    /// Stage two runs only after the daemon has durably persisted the raw image.
    fn complete_capture(&self, _result: CaptureResult) -> AdapterFuture<'_, Vec<CaptureImage>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn set_review_mode(&self, on: bool) -> AdapterFuture<'_, ()>;
    fn push_bindings(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, ()>;
    fn control(&self, session: Id, request: ControlRequest) -> AdapterFuture<'_, ControlState>;
}
pub trait DesktopHotkeys: Send + Sync {
    fn apply(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, ()>;
    fn check_conflicts(&self, bindings: Vec<Binding>) -> AdapterFuture<'_, Vec<BindingIssue>>;
    fn clear(&self) -> AdapterFuture<'_, ()>;
}
