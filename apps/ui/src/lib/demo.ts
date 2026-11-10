import type {
  AgentProfile,
  Annotation,
  Batch,
  ServerEnvelope,
  ServerEvent,
  Settings,
  Snapshot,
  Task,
  TaskStatus,
} from "@zuruf/protocol";
import type { LinkState, Transport } from "./api";
import { RequestError } from "./api";
import { defaultBindings } from "./chord";

/**
 * In-browser stand-in for the daemon (open the UI with ?demo).
 * It speaks the same protocol, so every screen can be designed and tested
 * without a running backend. Not shipped behaviour — only a design aid.
 */

const now = () => new Date().toISOString();
const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
let counter = 0;
const id = (prefix: string) => `${prefix}-${(++counter).toString().padStart(4, "0")}`;

function placeholderShot(kind: Annotation["type"]): string {
  const mark =
    kind === "dom"
      ? `<rect x="96" y="70" width="92" height="26" rx="5" fill="none" stroke="#8fa8ff" stroke-width="3"/>`
      : kind === "region"
        ? `<rect x="40" y="40" width="150" height="84" rx="3" fill="none" stroke="#ff5a36" stroke-width="3" stroke-dasharray="7 5"/>`
        : `<circle cx="204" cy="92" r="22" fill="none" stroke="#ff5a36" stroke-width="3"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="200" viewBox="0 0 320 200">
    <rect width="320" height="200" fill="#f4f3ef"/>
    <rect width="320" height="26" fill="#ffffff"/><rect x="12" y="9" width="54" height="8" rx="4" fill="#1d1d1b"/>
    <rect x="24" y="48" width="130" height="10" rx="5" fill="#c9c8c2"/>
    <rect x="24" y="70" width="60" height="26" rx="5" fill="#e3e2dc"/><rect x="96" y="70" width="92" height="26" rx="5" fill="#1d1d1b"/>
    <rect x="24" y="116" width="272" height="60" rx="6" fill="#ffffff" stroke="#e3e2dc"/>
    <rect x="36" y="130" width="120" height="8" rx="4" fill="#c9c8c2"/><rect x="36" y="148" width="200" height="8" rx="4" fill="#e3e2dc"/>
    ${mark}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

function createDemoSnapshot(): Snapshot {
  const projectId = "project-demo";
  const reviewId = "review-demo";
  const sessionId = "session-demo";
  const draftId = "batch-draft";

  const profiles: AgentProfile[] = [
    {
      id: "profile-codex",
      name: "Codex · gründlich",
      providerInstanceId: "codex",
      model: "gpt-6.1-sol",
      options: { reasoningEffort: "high" },
      workspaceStrategy: "new_worktree",
      slot: 1,
    },
    {
      id: "profile-claude",
      name: "Claude · schnell",
      providerInstanceId: "claudeAgent",
      model: "claude-sonnet-5-5",
      options: { effort: "medium" },
      workspaceStrategy: "new_worktree",
      slot: 2,
    },
  ];

  const settings: Settings = {
    bindings: defaultBindings(),
    capture: { ringColor: "#ff5a36", ringRadius: 28, keepRawScreenshot: true, mode: "new_entry" },
    speech: {
      enabled: true,
      backend: "continuous",
      model: "large-v3-turbo",
      language: "de",
      attach: "after",
      keepAudio: false,
      voxtypePath: "/usr/bin/voxtype",
    },
    redaction: { stripQuery: false, patterns: ["token", "session", "secret"] },
    t3: { cliPath: "~/.local/bin/t3", pollSeconds: 3 },
    ui: { theme: "system", language: "de", openPanelOnReviewStart: true },
    dataDir: "~/.local/share/zuruf",
  };

  const annotation = (order: number, type: Annotation["type"], transcript: string, extra: Partial<Annotation>) =>
    ({
      id: id("ann"),
      batchId: draftId,
      reviewId,
      projectId,
      sessionId,
      type,
      order,
      url: "http://localhost:3000/angebote",
      route: "/angebote",
      pageTitle: "Angebote",
      assets:
        type === "text"
          ? []
          : [{ id: id("asset"), kind: "screenshot_marked", path: `assets/ann-${order}.png`, url: placeholderShot(type) }],
      transcript,
      transcriptState: "ready",
      segmentIds: [],
      note: "",
      priority: "normal",
      tags: [],
      status: "open",
      createdAt: ago(3 - order),
      updatedAt: ago(3 - order),
      ...extra,
    }) satisfies Annotation;

  const task = (
    seq: number,
    title: string,
    status: TaskStatus,
    profile: AgentProfile,
    extra: Partial<Task> = {},
  ): Task => ({
    id: id("task"),
    batchId: `batch-${seq}`,
    projectId,
    title,
    status,
    profile,
    providerDisplayName: profile.providerInstanceId === "codex" ? "Codex" : "Claude",
    threadId: status === "finalizing_transcript" || status === "queued" ? null : `mcp:thread-${seq}`,
    threadUrl: null,
    workspacePath: `~/.t3/worktrees/main/zuruf-${seq}`,
    branch: `zuruf/batch-${seq}`,
    workspaceConflict: false,
    markedDone: false,
    annotationCount: 2,
    createdAt: ago(40 - seq * 6),
    dispatchedAt: ago(39 - seq * 6),
    ...extra,
  });

  return {
    status: {
      protocolVersion: 1,
      daemonVersion: "0.1.0-demo",
      reviewMode: true,
      activeProjectId: projectId,
      activeReviewId: reviewId,
      activeBatchId: draftId,
      activeSessionId: sessionId,
      pendingAction: null,
      speech: { state: "recording", pendingSegments: 0 },
      t3: {
        state: "connected",
        version: "0.1.7",
        transport: "mcp",
        capabilities: {
          modelOptions: true,
          existingWorktree: true,
          idempotentLaunch: true,
          interrupt: true,
          attachments: false,
          push: false,
        },
      },
    },
    projects: [
      {
        id: projectId,
        name: "Beispielprojekt",
        repositoryPath: "~/Projects/Beispielprojekt",
        baseUrl: "http://localhost:3000",
        t3ProjectId: "t3-example",
        defaultProfileId: "profile-codex",
        createdAt: ago(900),
        updatedAt: ago(900),
      },
    ],
    profiles,
    sessions: [
      {
        id: sessionId,
        projectId,
        name: "Hauptsitzung",
        mode: "shared",
        state: "running",
        agentAccess: true,
        control: { owner: "human", settling: false },
        tabs: [
          { id: "tab-1", sessionId, url: "http://localhost:3000/angebote", title: "Angebote", active: true, scriptable: true },
          { id: "tab-2", sessionId, url: "http://localhost:3000/dashboard", title: "Übersicht", active: false, scriptable: true },
        ],
        createdAt: ago(600),
      },
      {
        id: "session-iso",
        projectId,
        name: "Testkunde",
        mode: "isolated",
        state: "stopped",
        agentAccess: false,
        control: { owner: "none", settling: false },
        tabs: [],
        createdAt: ago(300),
      },
    ],
    reviews: [{ id: reviewId, projectId, name: "Review 10. November", createdAt: ago(60) }],
    batches: [
      {
        id: draftId,
        reviewId,
        projectId,
        seq: 5,
        state: "draft",
        title: "",
        note: "",
        profileId: "profile-codex",
        createdAt: ago(4),
      },
    ],
    annotations: [
      annotation(1, "dom", "Der Button soll nach dem Speichern eine Bestätigung zeigen.", {
        dom: {
          locators: [
            {
              kind: "testid",
              value: "save-button",
              playwright: "getByTestId('save-button')",
              stable: true,
            },
            {
              kind: "role",
              value: "button „Speichern“",
              playwright: "getByRole('button', { name: 'Speichern' })",
              stable: true,
            },
          ],
          tag: "button",
          role: "button",
          name: "Speichern",
          text: "Speichern",
          attributes: { "data-testid": "save-button", type: "submit" },
          boundingBox: { x: 96, y: 70, width: 92, height: 26 },
          framePath: [],
          shadowPath: [],
          children: [],
        },
      }),
      annotation(2, "region", "Der Abstand zwischen den Karten ist zu eng.", {}),
      annotation(3, "viewport", "Hier fehlt ein Leerzustand, wenn der Filter nichts findet.", {
        url: "http://localhost:3000/angebote?filter=offen",
      }),
    ],
    segments: [],
    tasks: [
      task(4, "Kopfzeile der Tabelle fixieren", "finalizing_transcript", profiles[0]!, { dispatchedAt: null }),
      task(3, "Bestätigung nach Speichern, Kartenabstand", "running", profiles[0]!, {
        lastMessage: "Ich passe die Toast-Komponente an und prüfe danach den Abstand im Raster.",
      }),
      task(2, "Filterleiste auf Mobil", "needs_approval", profiles[1]!, {
        pendingRequest: "pnpm add @radix-ui/react-popover",
      }),
      task(1, "Leerzustand Angebotsliste", "completed", profiles[1]!, {
        finishedAt: ago(12),
        lastMessage:
          "Leerzustand ergänzt (src/app/angebote/empty-state.tsx). Im Browser geprüft: erscheint bei leerem Filter, verschwindet nach Zurücksetzen.",
      }),
      task(0, "Anmeldeseite Fokusreihenfolge", "unknown_remote_state", profiles[0]!, {
        threadId: null,
        error: "Zeitüberschreitung beim Anlegen des Threads.",
      }),
    ],
    settings,
  };
}

export function createDemoTransport(): Transport {
  const snapshot = createDemoSnapshot();
  const listeners = new Set<(event: ServerEnvelope) => void>();
  let seq = 0;

  const emit = (event: ServerEvent) => {
    const envelope = { ...structuredClone(event), seq: ++seq } as ServerEnvelope;
    for (const listener of listeners) listener(envelope);
  };
  const statusChanged = () => emit({ type: "status.changed", payload: snapshot.status });

  // Keep the waveform alive while "recording".
  let phase = 0;
  window.setInterval(() => {
    if (snapshot.status.speech.state !== "recording" || listeners.size === 0) return;
    phase += 0.5;
    const level = Math.max(0.04, Math.abs(Math.sin(phase) * Math.sin(phase * 0.37)) * (0.5 + Math.random() * 0.5));
    emit({ type: "speech.level", payload: { level } });
  }, 70);

  function advance(task: Task, steps: [number, TaskStatus, string?][]) {
    for (const [delay, status, message] of steps) {
      window.setTimeout(() => {
        task.status = status;
        if (message) task.lastMessage = message;
        if (status === "running") {
          task.threadId = `mcp:${task.id}`;
          task.dispatchedAt = now();
        }
        if (status === "completed") task.finishedAt = now();
        emit({ type: "task.upserted", payload: task });
      }, delay);
    }
  }

  function dispatch(batchId: string, profileId?: string | null) {
    const sealed = snapshot.batches.find((entry) => entry.id === batchId);
    if (!sealed || sealed.state !== "draft") throw new RequestError(409, "batch_not_draft", "Batch ist nicht offen.");
    const annotations = snapshot.annotations.filter((entry) => entry.batchId === batchId);
    if (annotations.length === 0 && !sealed.note.trim()) {
      throw new RequestError(422, "batch_empty", "Nichts zu senden.");
    }
    const profile =
      snapshot.profiles.find((entry) => entry.id === (profileId ?? sealed.profileId)) ?? snapshot.profiles[0]!;

    sealed.state = "sealed";
    sealed.sealedAt = now();
    const task: Task = {
      id: id("task"),
      batchId,
      projectId: sealed.projectId,
      title: annotations[0]?.transcript.slice(0, 60) || sealed.note.slice(0, 60) || `Batch ${sealed.seq}`,
      status: "finalizing_transcript",
      profile,
      providerDisplayName: profile.providerInstanceId === "codex" ? "Codex" : "Claude",
      workspaceConflict: false,
      markedDone: false,
      annotationCount: annotations.length,
      createdAt: now(),
    };
    sealed.taskId = task.id;
    snapshot.tasks.push(task);

    const activeBatch: Batch = {
      id: id("batch"),
      reviewId: sealed.reviewId,
      projectId: sealed.projectId,
      seq: sealed.seq + 1,
      state: "draft",
      title: "",
      note: "",
      profileId: sealed.profileId,
      createdAt: now(),
    };
    snapshot.batches.push(activeBatch);
    snapshot.status.activeBatchId = activeBatch.id;

    emit({ type: "batch.upserted", payload: sealed });
    emit({ type: "batch.upserted", payload: activeBatch });
    emit({ type: "task.upserted", payload: task });
    statusChanged();
    advance(task, [
      [900, "queued"],
      [1500, "dispatching"],
      [2600, "running", "Ich lese das Manifest und die Screenshots."],
      [9000, "completed", "Alle Punkte umgesetzt und im Browser geprüft."],
    ]);
    return { sealedBatch: sealed, task, activeBatch };
  }

  const captureTexts = [
    "Die Überschrift ist hier abgeschnitten.",
    "Dieser Bereich wirkt zu gedrängt.",
    "Das Element braucht einen Hover-Zustand.",
  ];

  function capture(type: Annotation["type"]) {
    const batchId = snapshot.status.activeBatchId!;
    const order = snapshot.annotations.filter((entry) => entry.batchId === batchId).length + 1;
    const created: Annotation = {
      id: id("ann"),
      batchId,
      reviewId: snapshot.status.activeReviewId!,
      projectId: snapshot.status.activeProjectId!,
      type,
      order,
      url: "http://localhost:3000/angebote",
      route: "/angebote",
      pageTitle: "Angebote",
      assets: [{ id: id("asset"), kind: "screenshot_marked", path: "", url: placeholderShot(type) }],
      transcript: "",
      transcriptState: snapshot.status.speech.state === "recording" ? "recording" : "none",
      segmentIds: [],
      note: "",
      priority: "normal",
      tags: [],
      status: "open",
      createdAt: now(),
      updatedAt: now(),
    };
    snapshot.annotations.push(created);
    emit({ type: "annotation.upserted", payload: created });
    if (created.transcriptState === "recording") {
      window.setTimeout(() => {
        created.transcriptState = "ready";
        created.transcript = captureTexts[order % captureTexts.length]!;
        emit({ type: "annotation.upserted", payload: created });
      }, 2600);
    }
  }

  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const data = (body ?? {}) as Record<string, unknown>;
    const [, resource, resourceId, verb] = path.split("?")[0]!.split("/");
    const reply = (value: unknown) => structuredClone(value) as T;

    if (resource === "state") return reply(snapshot);

    if (resource === "actions") {
      switch (resourceId) {
        case "capture.viewport":
          capture("viewport");
          break;
        case "capture.region":
          capture("region");
          break;
        case "capture.dom":
          capture("dom");
          break;
        case "speech.toggle":
          snapshot.status.speech.state = snapshot.status.speech.state === "recording" ? "idle" : "recording";
          statusChanged();
          break;
        case "review.toggle":
          snapshot.status.reviewMode = !snapshot.status.reviewMode;
          statusChanged();
          break;
        case "batch.dispatch":
          dispatch(snapshot.status.activeBatchId!);
          break;
        case "control.takeover": {
          const session = snapshot.sessions.find((entry) => entry.id === snapshot.status.activeSessionId);
          if (session) {
            session.control = { owner: "human", settling: false };
            emit({ type: "session.upserted", payload: session });
          }
          break;
        }
        default:
          break;
      }
      return reply({ ok: true });
    }

    if (resource === "review-mode") {
      snapshot.status.reviewMode = Boolean(data.on);
      statusChanged();
      return reply(snapshot.status);
    }

    if (resource === "batches" && resourceId) {
      if (verb === "dispatch") return reply(dispatch(resourceId, data.profileId as string | undefined));
      const target = snapshot.batches.find((entry) => entry.id === resourceId)!;
      Object.assign(target, data);
      emit({ type: "batch.upserted", payload: target });
      return reply(target);
    }

    if (resource === "annotations") {
      if (method === "POST") {
        const batchId = data.batchId as string;
        const created: Annotation = {
          id: id("ann"),
          batchId,
          reviewId: snapshot.status.activeReviewId!,
          projectId: snapshot.status.activeProjectId!,
          type: "text",
          order: snapshot.annotations.filter((entry) => entry.batchId === batchId).length + 1,
          assets: [],
          transcript: "",
          transcriptState: "none",
          segmentIds: [],
          note: String(data.note ?? ""),
          priority: "normal",
          tags: [],
          status: "open",
          createdAt: now(),
          updatedAt: now(),
        };
        snapshot.annotations.push(created);
        emit({ type: "annotation.upserted", payload: created });
        return reply(created);
      }
      const index = snapshot.annotations.findIndex((entry) => entry.id === resourceId);
      const target = snapshot.annotations[index]!;
      if (method === "DELETE") {
        snapshot.annotations.splice(index, 1);
        emit({ type: "annotation.removed", payload: { id: target.id, batchId: target.batchId } });
        return reply({});
      }
      Object.assign(target, data, { updatedAt: now() });
      emit({ type: "annotation.upserted", payload: target });
      return reply(target);
    }

    if (resource === "tasks" && resourceId) {
      const target = snapshot.tasks.find((entry) => entry.id === resourceId)!;
      if (verb === "events") {
        return reply([
          { id: id("ev"), taskId: target.id, at: target.createdAt, kind: "status", status: "queued" },
          { id: id("ev"), taskId: target.id, at: target.dispatchedAt ?? target.createdAt, kind: "status", status: "running" },
        ]);
      }
      if (method === "PATCH") Object.assign(target, data);
      if (verb === "retry" || verb === "resolve") {
        target.error = null;
        if (data.resolution === "adopt_thread") {
          target.status = "running";
          target.threadId = String(data.threadId ?? "");
        } else {
          target.status = "queued";
          advance(target, [
            [800, "dispatching"],
            [1800, "running", "Ich lese das Manifest und die Screenshots."],
          ]);
        }
      }
      if (verb === "cancel") target.status = "cancelled";
      if (verb === "follow-up") {
        target.status = "running";
        target.markedDone = false;
        advance(target, [[5000, "completed", "Nachtrag umgesetzt."]]);
      }
      emit({ type: "task.upserted", payload: target });
      return reply(target);
    }

    if (resource === "sessions" && resourceId) {
      const target = snapshot.sessions.find((entry) => entry.id === resourceId)!;
      if (verb === "start") target.state = "running";
      if (verb === "stop") target.state = "stopped";
      if (verb === "control") {
        const action = data.action as string;
        if (action === "takeover") target.control = { owner: "human", settling: false };
        if (action === "release") target.control = { owner: "none", settling: false };
        if (action === "grant") target.control = { owner: "agent", agentId: String(data.agentId ?? "Codex"), settling: false };
      }
      if (method === "PATCH") Object.assign(target, data);
      emit({ type: "session.upserted", payload: target });
      return reply(target);
    }

    if (resource === "settings") {
      if (resourceId === "bindings") return reply([]);
      if (method === "PUT") {
        snapshot.settings = body as Settings;
        emit({ type: "settings.changed", payload: snapshot.settings });
      }
      return reply(snapshot.settings);
    }

    if (resource === "profiles") {
      if (method === "DELETE") {
        snapshot.profiles = snapshot.profiles.filter((entry) => entry.id !== resourceId);
        emit({ type: "profile.removed", payload: { id: resourceId! } });
        return reply({});
      }
      const existing = snapshot.profiles.find((entry) => entry.id === resourceId);
      const saved = existing ? Object.assign(existing, data) : ({ ...data, id: id("profile") } as AgentProfile);
      if (!existing) snapshot.profiles.push(saved);
      emit({ type: "profile.upserted", payload: saved });
      return reply(saved);
    }

    if (resource === "t3" && resourceId === "catalog") {
      return reply({
        fetchedAt: now(),
        runtimeModes: ["approval-required", "auto-accept-edits", "full-access"],
        interactionModes: ["default", "plan"],
        providers: [
          {
            instanceId: "codex",
            displayName: "Codex",
            enabled: true,
            constraints: [],
            omittedModels: 4,
            models: [
              { id: "gpt-6.1-sol", options: [{ id: "reasoningEffort", values: ["low", "medium", "high", "xhigh"], default: "low" }] },
              { id: "gpt-6-luna", options: [{ id: "reasoningEffort", values: ["low", "medium", "high"], default: "medium" }] },
            ],
          },
          {
            instanceId: "claudeAgent",
            displayName: "Claude",
            enabled: true,
            constraints: [],
            omittedModels: 10,
            models: [
              { id: "claude-opus-5-5", label: "Claude Opus 5.5", options: [{ id: "effort", values: ["low", "medium", "high"], default: "medium" }] },
              { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5", options: [{ id: "effort", values: ["low", "medium", "high"], default: "high" }] },
            ],
          },
        ],
      });
    }

    return reply({});
  }

  return {
    request,
    subscribe(onEvent, onState: (state: LinkState) => void) {
      listeners.add(onEvent);
      onState("online");
      queueMicrotask(() => onEvent({ type: "snapshot", payload: structuredClone(snapshot), seq: ++seq }));
      return () => listeners.delete(onEvent);
    },
    asset: (path) => Promise.resolve(path),
  };
}
