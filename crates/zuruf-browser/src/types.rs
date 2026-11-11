use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocatorCandidate {
    pub kind: String,
    pub value: String,
    pub playwright: Option<String>,
    pub stable: bool,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomCapture {
    pub locators: Vec<LocatorCandidate>,
    pub tag: String,
    pub role: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub bounding_box: Rect,
    pub frame_path: Vec<String>,
    pub shadow_path: Vec<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chord {
    pub mods: Vec<String>,
    pub code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub action: String,
    pub scope: String,
    pub chord: Option<Chord>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: String,
    pub session_id: String,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub scriptable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlState {
    pub owner: String,
    pub agent_id: Option<String>,
    pub lease_expires_at: Option<String>,
    pub settling: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Viewport,
    Region,
    Dom,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub device_scale_factor: f64,
}
/// Something a page reported on its own: a console message, an uncaught exception or a
/// request that failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// `console`, `exception` or `network`.
    pub kind: String,
    /// `error` or `warning`.
    pub level: String,
    pub text: String,
    /// Where it came from: the script, or the address that failed to load.
    pub url: Option<String>,
    pub line: Option<u32>,
    /// How often the same report came in a row.
    pub count: u32,
    /// How long before the capture it was last reported.
    pub age_ms: u64,
}
#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub raw_png: Vec<u8>,
    pub marked_png: Vec<u8>,
    pub url: String,
    pub title: String,
    pub viewport: Viewport,
    pub scroll: Point,
    pub cursor: Point,
    pub rect: Option<Rect>,
    pub dom: Option<DomCapture>,
    pub tab: BrowserTab,
    /// What the page reported since its document loaded, oldest first.
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug, Clone)]
pub enum BrowserEvent {
    TabsChanged(Vec<BrowserTab>),
    /// The browser went away unexpectedly.
    Crashed,
    /// The user closed the session's window.
    Closed,
    ControlChanged(ControlState),
    /// A key press, picker result or other message from a page.
    Page(Box<crate::bridge::Envelope>),
}
