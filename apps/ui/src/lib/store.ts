import type {
  ActionId,
  AgentProfile,
  Annotation,
  Batch,
  Binding,
  BindingIssue,
  BrowserSession,
  ControlRequest,
  DispatchResponse,
  Notice,
  Project,
  ServerEnvelope,
  Settings,
  Snapshot,
  T3Catalog,
  Task,
  TaskEvent,
} from "@zuruf/protocol";
import { batch as transaction, createMemo, createSignal } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import type { LinkState, Transport } from "./api";

export interface Toast {
  id: number;
  notice: Notice;
}

interface State {
  ready: boolean;
  link: LinkState;
  snapshot: Snapshot | null;
  /** Live microphone level 0…1, updated ~15×/s; kept outside the snapshot to avoid churn. */
  level: number;
  partial: string;
  toasts: Toast[];
}

function upsert<T extends { id: string }>(list: T[], item: T) {
  const index = list.findIndex((entry) => entry.id === item.id);
  if (index === -1) list.push(item);
  else list[index] = item;
}

function removeById<T extends { id: string }>(list: T[], id: string) {
  const index = list.findIndex((entry) => entry.id === id);
  if (index !== -1) list.splice(index, 1);
}

export function createAppStore(transport: Transport) {
  const [state, setState] = createStore<State>({
    ready: false,
    link: "connecting",
    snapshot: null,
    level: 0,
    partial: "",
    toasts: [],
  });
  const [catalog, setCatalog] = createSignal<T3Catalog | null>(null);
  let toastId = 0;

  const mutate = (change: (snapshot: Snapshot) => void) =>
    setState(
      produce((draft) => {
        if (draft.snapshot) change(draft.snapshot);
      }),
    );

  function apply(event: ServerEnvelope) {
    switch (event.type) {
      case "snapshot":
        transaction(() => {
          setState("snapshot", reconcile(event.payload, { key: "id" }));
          setState("ready", true);
        });
        break;
      case "status.changed":
        setState("snapshot", "status", reconcile(event.payload));
        if (event.payload.speech.state !== "recording") setState({ level: 0, partial: "" });
        break;
      case "speech.level":
        setState("level", event.payload.level);
        break;
      case "speech.partial":
        setState("partial", event.payload.text);
        break;
      case "project.upserted":
        mutate((s) => upsert(s.projects, event.payload));
        break;
      case "project.removed":
        mutate((s) => removeById(s.projects, event.payload.id));
        break;
      case "profile.upserted":
        mutate((s) => upsert(s.profiles, event.payload));
        break;
      case "profile.removed":
        mutate((s) => removeById(s.profiles, event.payload.id));
        break;
      case "session.upserted":
        mutate((s) => upsert(s.sessions, event.payload));
        break;
      case "session.removed":
        mutate((s) => removeById(s.sessions, event.payload.id));
        break;
      case "review.upserted":
        mutate((s) => upsert(s.reviews, event.payload));
        break;
      case "batch.upserted":
        mutate((s) => upsert(s.batches, event.payload));
        break;
      case "annotation.upserted":
        mutate((s) => upsert(s.annotations, event.payload));
        break;
      case "annotation.removed":
        mutate((s) => removeById(s.annotations, event.payload.id));
        break;
      case "segment.upserted":
        mutate((s) => upsert(s.segments, event.payload));
        break;
      case "task.upserted":
        mutate((s) => upsert(s.tasks, event.payload));
        break;
      case "settings.changed":
        setState("snapshot", "settings", reconcile(event.payload));
        break;
      case "app.open_requested":
        if (location.hash !== `#${event.payload.view}`) {
          location.hash = event.payload.view;
          location.reload();
        }
        break;
      case "notice":
        pushToast(event.payload);
        break;
      default:
        // Unknown events are ignored by contract.
        break;
    }
  }

  function pushToast(notice: Notice) {
    const id = ++toastId;
    setState("toasts", (list) => [...list.slice(-2), { id, notice }]);
    window.setTimeout(() => dismissToast(id), notice.level === "error" ? 8000 : 4000);
  }

  function dismissToast(id: number) {
    setState("toasts", (list) => list.filter((toast) => toast.id !== id));
  }

  /** Runs a request and turns failures into a toast instead of an unhandled rejection. */
  async function run<T>(work: () => Promise<T>): Promise<T | undefined> {
    try {
      return await work();
    } catch (error) {
      const code = (error as { code?: string }).code ?? "request_failed";
      pushToast({ level: "error", code, detail: (error as Error).message });
      return undefined;
    }
  }

  const unsubscribe = transport.subscribe(apply, (link) => setState("link", link));

  // ─── derived views

  const snapshot = () => state.snapshot;
  const status = () => state.snapshot?.status;
  const settings = () => state.snapshot?.settings;

  const activeProject = createMemo<Project | undefined>(() =>
    state.snapshot?.projects.find((project) => project.id === state.snapshot?.status.activeProjectId),
  );
  const activeSession = createMemo<BrowserSession | undefined>(() =>
    state.snapshot?.sessions.find((session) => session.id === state.snapshot?.status.activeSessionId),
  );
  const draft = createMemo<Batch | undefined>(() =>
    state.snapshot?.batches.find((entry) => entry.id === state.snapshot?.status.activeBatchId),
  );
  const draftAnnotations = createMemo<Annotation[]>(() => {
    const id = draft()?.id;
    if (!id || !state.snapshot) return [];
    return state.snapshot.annotations.filter((entry) => entry.batchId === id).sort((a, b) => a.order - b.order);
  });
  const draftProfile = createMemo<AgentProfile | undefined>(() => {
    const profiles = state.snapshot?.profiles ?? [];
    const wanted = draft()?.profileId ?? activeProject()?.defaultProfileId;
    return profiles.find((profile) => profile.id === wanted) ?? profiles[0];
  });
  const tasks = createMemo<Task[]>(() =>
    [...(state.snapshot?.tasks ?? [])].sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
  );
  const bindingFor = (action: ActionId, scope: Binding["scope"] = "page") =>
    state.snapshot?.settings.bindings.find((binding) => binding.action === action && binding.scope === scope)
      ?.chord ?? null;

  // ─── commands

  const api = transport.request.bind(transport);

  const commands = {
    action: (id: ActionId) => run(() => api<{ ok: true }>("POST", `/actions/${id}`, {})),
    setReviewMode: (on: boolean) => run(() => api("POST", "/review-mode", { on })),
    setActive: (ids: { projectId?: string; reviewId?: string; sessionId?: string }) =>
      run(() => api("POST", "/active", ids)),

    dispatch: (batchId: string, body: { profileId?: string; followUpTaskId?: string } = {}) =>
      run(() => api<DispatchResponse>("POST", `/batches/${batchId}/dispatch`, body)),
    patchBatch: (id: string, body: Partial<Pick<Batch, "title" | "note" | "profileId">>) =>
      run(() => api<Batch>("PATCH", `/batches/${id}`, body)),

    addTextAnnotation: (batchId: string, note: string) =>
      run(() => api<Annotation>("POST", "/annotations", { batchId, type: "text", note })),
    patchAnnotation: (
      id: string,
      body: Partial<Pick<Annotation, "transcript" | "note" | "priority" | "tags" | "status" | "order" | "batchId">>,
    ) => run(() => api<Annotation>("PATCH", `/annotations/${id}`, body)),
    removeAnnotation: (id: string) => run(() => api("DELETE", `/annotations/${id}`)),

    patchTask: (id: string, body: { markedDone: boolean }) => run(() => api<Task>("PATCH", `/tasks/${id}`, body)),
    retryTask: (id: string) => run(() => api<Task>("POST", `/tasks/${id}/retry`, {})),
    cancelTask: (id: string) => run(() => api<Task>("POST", `/tasks/${id}/cancel`, {})),
    resolveTask: (id: string, body: { resolution: "adopt_thread" | "not_sent"; threadId?: string }) =>
      run(() => api<Task>("POST", `/tasks/${id}/resolve`, body)),
    followUp: (id: string, body: { text?: string; annotationIds?: string[] }) =>
      run(() => api<Task>("POST", `/tasks/${id}/follow-up`, body)),
    taskEvents: (id: string) => run(() => api<TaskEvent[]>("GET", `/tasks/${id}/events`)),

    createProject: (body: Pick<Project, "name" | "repositoryPath" | "baseUrl">) =>
      run(() => api<Project>("POST", "/projects", body)),
    patchProject: (id: string, body: Partial<Project>) => run(() => api<Project>("PATCH", `/projects/${id}`, body)),
    saveProfile: (profile: Omit<AgentProfile, "id"> & { id?: string }) =>
      run(() =>
        profile.id
          ? api<AgentProfile>("PATCH", `/profiles/${profile.id}`, profile)
          : api<AgentProfile>("POST", "/profiles", profile),
      ),
    removeProfile: (id: string) => run(() => api("DELETE", `/profiles/${id}`)),
    loadCatalog: async (refresh = false) => {
      const result = await run(() => api<T3Catalog>("GET", `/t3/catalog${refresh ? "?refresh=1" : ""}`));
      if (result) setCatalog(result);
      return result;
    },
    reconnectT3: () => run(() => api("POST", "/t3/reconnect", {})),

    createSession: (body: Pick<BrowserSession, "projectId" | "name" | "mode" | "agentAccess">) =>
      run(() => api<BrowserSession>("POST", "/sessions", body)),
    sessionCommand: (id: string, command: "start" | "stop" | "duplicate") =>
      run(() => api<BrowserSession>("POST", `/sessions/${id}/${command}`, {})),
    removeSession: (id: string) => run(() => api("DELETE", `/sessions/${id}`)),
    control: (id: string, body: ControlRequest) =>
      run(() => api<BrowserSession>("POST", `/sessions/${id}/control`, body)),

    saveSettings: (next: Settings) => run(() => api<Settings>("PUT", "/settings", next)),
    checkBindings: (bindings: Binding[]) =>
      run(() => api<BindingIssue[]>("POST", "/settings/bindings/check", bindings)),
  };

  return {
    state,
    snapshot,
    status,
    settings,
    catalog,
    activeProject,
    activeSession,
    draft,
    draftAnnotations,
    draftProfile,
    tasks,
    bindingFor,
    commands,
    asset: transport.asset.bind(transport),
    pushToast,
    dismissToast,
    dispose: unsubscribe,
  };
}

export type AppStore = ReturnType<typeof createAppStore>;
