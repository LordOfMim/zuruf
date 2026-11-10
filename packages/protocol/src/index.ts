/**
 * Zuruf wire protocol v1 — the single source of truth for everything that
 * crosses the boundary between `zuruf daemon` (Rust) and its clients
 * (UI, side panel, extension, CLI).
 *
 * Conventions:
 * - JSON, camelCase keys. Rust mirrors these with `#[serde(rename_all = "camelCase")]`.
 * - Ids are UUIDv7 strings. Timestamps are RFC 3339 UTC strings.
 * - Optional fields are omitted or `null`; clients treat both the same.
 * - Enums are lowercase snake_case strings.
 * - Unknown fields and unknown event types must be ignored by clients.
 */

export const PROTOCOL_VERSION = 1;

export type Id = string;
export type Timestamp = string;

// ───────────────────────────── Projects & agent profiles

export interface Project {
  id: Id;
  name: string;
  repositoryPath: string;
  baseUrl: string;
  /** Id of the matching project in the connected T3 instance, if resolved. */
  t3ProjectId?: string | null;
  defaultProfileId?: Id | null;
  createdAt: Timestamp;
  updatedAt: Timestamp;
}

export type WorkspaceStrategy = "new_worktree" | "existing_worktree" | "shared_root";

export interface AgentProfile {
  id: Id;
  name: string;
  providerInstanceId: string;
  model: string;
  /** Provider options exactly as offered by the T3 catalog, e.g. { reasoningEffort: "high" }. */
  options: Record<string, string | boolean>;
  runtimeMode?: string | null;
  interactionMode?: string | null;
  workspaceStrategy: WorkspaceStrategy;
  /** For new_worktree. Defaults to HEAD. */
  baseRef?: string | null;
  /** For existing_worktree. */
  worktreePath?: string | null;
  /** 1–3: reachable through the profile.slotN shortcut actions. */
  slot?: number | null;
}

/** Live catalog from the connected T3 instance. Never a static list. */
export interface T3Catalog {
  fetchedAt: Timestamp;
  providers: T3Provider[];
  runtimeModes: string[];
  interactionModes: string[];
}

export interface T3Provider {
  instanceId: string;
  displayName: string;
  enabled: boolean;
  constraints: string[];
  models: T3Model[];
  /** Models exist beyond this list; use the search parameter. */
  omittedModels: number;
}

export interface T3Model {
  id: string;
  label?: string | null;
  options: { id: string; values: (string | boolean)[]; default?: string | boolean | null }[];
}

export interface T3ProjectRef {
  id: string;
  title: string;
  workspaceRoot: string;
}

export interface T3Connection {
  state: "disconnected" | "connecting" | "connected" | "error";
  version?: string | null;
  /** Which interface is in use. "cli" is the limited fallback. */
  transport?: "mcp" | "cli" | null;
  /** Feature flags found by capability detection. */
  capabilities: {
    modelOptions: boolean;
    existingWorktree: boolean;
    idempotentLaunch: boolean;
    interrupt: boolean;
    attachments: boolean;
    push: boolean;
  };
  error?: string | null;
}

// ───────────────────────────── Browser sessions

export type SessionMode = "shared" | "isolated" | "observer";

export interface ControlState {
  owner: "none" | "human" | "agent";
  agentId?: string | null;
  leaseExpiresAt?: Timestamp | null;
  /** True while a takeover waits for in-flight agent commands to finish. */
  settling: boolean;
}

export interface BrowserTab {
  id: Id;
  sessionId: Id;
  url: string;
  title: string;
  active: boolean;
  /** False on chrome://, PDF viewer and other pages the overlay cannot reach. */
  scriptable: boolean;
}

export interface BrowserSession {
  id: Id;
  projectId: Id;
  name: string;
  mode: SessionMode;
  state: "stopped" | "starting" | "running" | "crashed";
  /** Whether agents may request control of this session at all. */
  agentAccess: boolean;
  control: ControlState;
  tabs: BrowserTab[];
  createdAt: Timestamp;
}

// ───────────────────────────── Reviews, batches, annotations

export interface Review {
  id: Id;
  projectId: Id;
  name: string;
  createdAt: Timestamp;
}

/** A batch is "draft" while it collects annotations and "sealed" forever after Shift+N. */
export interface Batch {
  id: Id;
  reviewId: Id;
  projectId: Id;
  /** Running number within the review, for humans. */
  seq: number;
  state: "draft" | "sealed" | "discarded";
  title: string;
  /** Free text that belongs to the batch as a whole. */
  note: string;
  /** Profile chosen for this batch; frozen into the task when sealed. */
  profileId?: Id | null;
  taskId?: Id | null;
  createdAt: Timestamp;
  sealedAt?: Timestamp | null;
}

export type AnnotationType = "viewport" | "region" | "dom" | "text";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Asset {
  id: Id;
  kind: "screenshot" | "screenshot_marked" | "crop" | "audio";
  /** Path relative to the batch directory, e.g. "assets/ann-001.png". */
  path: string;
  width?: number | null;
  height?: number | null;
  /** Daemon URL path to fetch the bytes, e.g. "/api/v1/assets/<id>". */
  url: string;
}

export interface LocatorCandidate {
  kind: "testid" | "role" | "text" | "css" | "xpath";
  value: string;
  /** Ready-to-paste Playwright expression, e.g. getByRole('button', { name: 'Speichern' }). */
  playwright?: string | null;
  stable: boolean;
  reason?: string | null;
}

export interface DomCapture {
  locators: LocatorCandidate[];
  tag: string;
  role?: string | null;
  name?: string | null;
  text?: string | null;
  /** Allow-listed attributes only; values, tokens and secrets are never stored. */
  attributes: Record<string, string>;
  boundingBox: Rect;
  framePath: string[];
  shadowPath: string[];
  parent?: string | null;
  children: string[];
}

export interface Annotation {
  id: Id;
  batchId: Id;
  reviewId: Id;
  projectId: Id;
  sessionId?: Id | null;
  tabId?: Id | null;
  type: AnnotationType;
  /** Position within the batch. */
  order: number;
  url?: string | null;
  route?: string | null;
  pageTitle?: string | null;
  viewport?: { width: number; height: number; deviceScaleFactor: number } | null;
  scroll?: { x: number; y: number } | null;
  cursor?: { x: number; y: number } | null;
  /** Region rectangle in viewport CSS pixels. */
  rect?: Rect | null;
  dom?: DomCapture | null;
  assets: Asset[];
  /** Editable transcript; starts as the joined text of the linked segments. */
  transcript: string;
  transcriptState: "none" | "recording" | "transcribing" | "ready" | "failed";
  segmentIds: Id[];
  /** Typed comment (action comment.text). */
  note: string;
  priority: "low" | "normal" | "high";
  tags: string[];
  status: "open" | "in_progress" | "done" | "wont_fix";
  git?: { commit?: string | null; branch?: string | null } | null;
  createdAt: Timestamp;
  updatedAt: Timestamp;
}

export interface TranscriptSegment {
  id: Id;
  batchId: Id;
  /** Null: speech that belongs to the batch as a whole. */
  annotationId?: Id | null;
  startedAt: Timestamp;
  endedAt?: Timestamp | null;
  state: "recording" | "transcribing" | "ready" | "empty" | "failed";
  /** Text as recognised; never overwritten by edits. */
  originalText: string;
}

// ───────────────────────────── Tasks (one per sealed batch)

export type TaskStatus =
  | "finalizing_transcript"
  | "queued"
  | "dispatching"
  | "running"
  | "completed"
  | "needs_review"
  | "needs_approval"
  | "failed"
  | "cancelled"
  | "unknown_remote_state";

export interface Task {
  id: Id;
  batchId: Id;
  projectId: Id;
  title: string;
  status: TaskStatus;
  /** Original remote status string, kept for transparency. */
  remoteStatus?: string | null;
  /** Frozen copy of the profile used for dispatch. */
  profile: AgentProfile;
  providerDisplayName?: string | null;
  threadId?: string | null;
  threadUrl?: string | null;
  workspacePath?: string | null;
  branch?: string | null;
  /** Latest assistant message of the tracked run, possibly truncated. */
  lastMessage?: string | null;
  error?: string | null;
  /** What the agent is waiting for when status is needs_approval / needs_review. */
  pendingRequest?: string | null;
  /** Another running task writes to the same checkout. */
  workspaceConflict: boolean;
  markedDone: boolean;
  annotationCount: number;
  createdAt: Timestamp;
  dispatchedAt?: Timestamp | null;
  finishedAt?: Timestamp | null;
}

export interface TaskEvent {
  id: Id;
  taskId: Id;
  at: Timestamp;
  kind: "status" | "message" | "error" | "follow_up" | "retry" | "note";
  status?: TaskStatus | null;
  text?: string | null;
}

// ───────────────────────────── Speech

export interface SpeechState {
  state: "unavailable" | "idle" | "recording" | "transcribing" | "error" | "busy_external";
  /** Segments still being transcribed in the background. */
  pendingSegments: number;
  /** Set when the backend cannot record gaplessly; last measured gap. */
  lastGapMs?: number | null;
  error?: string | null;
}

// ───────────────────────────── Shortcuts & settings

export type ActionId =
  | "review.toggle"
  | "capture.viewport"
  | "capture.region"
  | "capture.dom"
  | "comment.text"
  | "speech.toggle"
  | "batch.dispatch"
  | "batch.send_to_thread"
  | "control.takeover"
  | "action.cancel"
  | "panel.toggle"
  | "app.open"
  | "profile.slot1"
  | "profile.slot2"
  | "profile.slot3";

export type Modifier = "ctrl" | "alt" | "shift" | "super";

/** Layout-independent chord: modifiers plus one KeyboardEvent.code, e.g. { mods: ["shift"], code: "KeyN" }. */
export interface Chord {
  mods: Modifier[];
  code: string;
}

export interface Binding {
  action: ActionId;
  /** page: inside the review browser, review mode only, never in text fields. global: desktop-wide via Hyprland, needs a modifier. */
  scope: "page" | "global";
  chord: Chord | null;
}

export interface BindingIssue {
  action: ActionId;
  scope: "page" | "global";
  kind: "duplicate" | "needs_modifier" | "taken_by_system" | "unsupported_key";
  /** Conflicting action or description of the system binding. */
  detail?: string | null;
}

export interface Settings {
  bindings: Binding[];
  capture: {
    ringColor: string;
    ringRadius: number;
    keepRawScreenshot: boolean;
    /** What W/S/D do to the current entry (spec §3). */
    mode: "new_entry" | "extend_open_entry";
  };
  speech: {
    enabled: boolean;
    /** continuous: own recording sliced at markers. daemon: Voxtype record start/stop with a gap. */
    backend: "continuous" | "daemon";
    model?: string | null;
    language?: string | null;
    /** Speech after a capture belongs to it ("after") or the speech before it ("before"). */
    attach: "after" | "before";
    keepAudio: boolean;
    voxtypePath: string;
  };
  redaction: {
    stripQuery: boolean;
    /** Extra attribute/param name patterns to drop. */
    patterns: string[];
  };
  t3: {
    cliPath: string;
    /** Poll interval for running tasks when push is unavailable. */
    pollSeconds: number;
  };
  ui: {
    theme: "system" | "dark" | "light";
    language: "de" | "en";
    openPanelOnReviewStart: boolean;
  };
  dataDir: string;
}

// ───────────────────────────── Snapshot & live events

export interface AppStatus {
  protocolVersion: number;
  daemonVersion: string;
  reviewMode: boolean;
  activeProjectId?: Id | null;
  activeReviewId?: Id | null;
  activeBatchId?: Id | null;
  activeSessionId?: Id | null;
  /** Action currently waiting for pointer input, e.g. a region drag. */
  pendingAction?: ActionId | null;
  speech: SpeechState;
  t3: T3Connection;
}

/** GET /api/v1/state and the first WebSocket event. */
export interface Snapshot {
  status: AppStatus;
  projects: Project[];
  profiles: AgentProfile[];
  sessions: BrowserSession[];
  reviews: Review[];
  /** The draft batch plus recently sealed ones of the active review. */
  batches: Batch[];
  /** Annotations of the batches above. */
  annotations: Annotation[];
  segments: TranscriptSegment[];
  tasks: Task[];
  settings: Settings;
}

export type ServerEvent =
  | { type: "snapshot"; payload: Snapshot }
  | { type: "status.changed"; payload: AppStatus }
  | { type: "speech.level"; payload: { level: number } }
  | { type: "speech.partial"; payload: { segmentId: Id; text: string } }
  | { type: "project.upserted"; payload: Project }
  | { type: "project.removed"; payload: { id: Id } }
  | { type: "profile.upserted"; payload: AgentProfile }
  | { type: "profile.removed"; payload: { id: Id } }
  | { type: "session.upserted"; payload: BrowserSession }
  | { type: "session.removed"; payload: { id: Id } }
  | { type: "review.upserted"; payload: Review }
  | { type: "batch.upserted"; payload: Batch }
  | { type: "annotation.upserted"; payload: Annotation }
  | { type: "annotation.removed"; payload: { id: Id; batchId: Id } }
  | { type: "segment.upserted"; payload: TranscriptSegment }
  | { type: "task.upserted"; payload: Task }
  | { type: "task.event"; payload: TaskEvent }
  | { type: "settings.changed"; payload: Settings }
  | { type: "notice"; payload: Notice };

/** Wire envelope on the events socket. `seq` increases by one per event per connection. */
export type ServerEnvelope = ServerEvent & { seq: number };

export interface Notice {
  level: "info" | "warning" | "error";
  /** Stable machine code, e.g. "speech.gap", "dispatch.failed", "capture.unscriptable_page". */
  code: string;
  /** Technical detail for the log line; UI copy comes from the code. */
  detail?: string | null;
  relatedId?: Id | null;
}

// ───────────────────────────── HTTP API (all under /api/v1, Bearer token)

export interface ApiError {
  error: { code: string; message: string; issues?: BindingIssue[] };
}

export interface DispatchRequest {
  /** Override the batch's profile for this dispatch. */
  profileId?: Id | null;
  /** Follow-up: send into this existing task's thread instead of a new one. */
  followUpTaskId?: Id | null;
}

export interface DispatchResponse {
  sealedBatch: Batch;
  task: Task;
  /** The new empty draft that is already active. */
  activeBatch: Batch;
}

export interface FollowUpRequest {
  text?: string;
  annotationIds?: Id[];
}

/** Resolve a task stuck in unknown_remote_state. */
export interface ResolveRequest {
  resolution: "adopt_thread" | "not_sent";
  threadId?: string;
}

export interface ControlRequest {
  action: "takeover" | "release" | "grant" | "revoke";
  agentId?: string;
  leaseSeconds?: number;
}

/**
 * Routes. `→` is the JSON response body; mutations also emit the matching events.
 *
 * GET    /state                                   → Snapshot
 * WS     /events                                  first client frame: { "type": "auth", "token": "…" }; then ServerEnvelope frames
 * POST   /auth/exchange        { code }           → { token }      one-time code minted by `zuruf open`
 *
 * POST   /actions/:actionId    {}                 → { ok: true }   same entry point for UI buttons, page hotkeys and the CLI
 * POST   /review-mode          { on: boolean }    → AppStatus
 * POST   /active               { projectId?, reviewId?, sessionId? } → AppStatus
 *
 * GET|POST /projects, PATCH|DELETE /projects/:id  → Project
 * GET|POST /profiles, PATCH|DELETE /profiles/:id  → AgentProfile
 * GET    /t3/catalog?search=&refresh=             → T3Catalog
 * GET    /t3/projects                             → T3ProjectRef[]
 * POST   /t3/reconnect                            → T3Connection
 *
 * POST   /sessions             { projectId, name, mode, agentAccess } → BrowserSession
 * PATCH|DELETE /sessions/:id
 * POST   /sessions/:id/start | /stop | /duplicate → BrowserSession
 * POST   /sessions/:id/tabs    { url }            → BrowserSession
 * POST   /sessions/:id/control ControlRequest     → BrowserSession
 *
 * POST   /reviews              { projectId, name } → Review
 * PATCH  /batches/:id          { title?, note?, profileId? } → Batch     drafts only
 * POST   /batches/:id/dispatch DispatchRequest    → DispatchResponse     returns immediately; never waits for T3
 * POST   /batches/:id/discard                     → Batch
 *
 * POST   /annotations          { batchId, type: "text", note } → Annotation
 * PATCH  /annotations/:id      { transcript?, note?, priority?, tags?, status?, order?, batchId? } → Annotation
 * DELETE /annotations/:id
 * GET    /assets/:id                              image bytes
 *
 * GET    /tasks?projectId=&limit=                 → Task[]
 * GET    /tasks/:id/events                        → TaskEvent[]
 * PATCH  /tasks/:id            { markedDone }     → Task
 * POST   /tasks/:id/retry | /cancel               → Task
 * POST   /tasks/:id/resolve    ResolveRequest     → Task
 * POST   /tasks/:id/follow-up  FollowUpRequest    → Task
 *
 * GET    /settings                                → Settings
 * PUT    /settings             Settings           → Settings | 422 ApiError with issues
 * POST   /settings/bindings/check  Binding[]      → BindingIssue[]
 */
export type Routes = never;
