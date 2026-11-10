import type { Project, T3ProjectRef } from "@zuruf/protocol";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { Button, Icon } from "../components/basics";
import type { AppStore } from "../lib/store";

const OTHER = "__other";

/** "http://localhost:3000/" → "localhost:3000" */
function address(url: string): string {
  return url.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

function folderName(path: string): string {
  return path.replace(/\/+$/, "").split("/").pop() ?? "";
}

/**
 * Add a project: pick one T3 already knows (or point at a folder) and say where it runs in the browser.
 * The address is the only thing the user has to type.
 */
export function ProjectForm(props: { store: AppStore; onDone?: () => void; onCancel?: () => void }) {
  const store = props.store;
  const connected = () => store.status()?.t3.state === "connected";
  const [refs] = createResource(connected, async (ok) => (ok ? ((await store.commands.t3Projects()) ?? []) : []));

  const known = createMemo(() => new Set(store.snapshot()?.projects.map((project) => project.repositoryPath)));
  const choices = createMemo(() => (refs() ?? []).filter((ref) => !known().has(ref.workspaceRoot)));

  const [picked, setPicked] = createSignal<string>("");
  const [path, setPath] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  // T3 titles repeat ("main" for every checkout), so ambiguous ones are named by their last two folders.
  const label = (entry: T3ProjectRef) => {
    const unique = choices().filter((other) => other.title === entry.title).length === 1;
    return unique ? entry.title : entry.workspaceRoot.replace(/\/+$/, "").split("/").slice(-2).join("/");
  };

  const manual = () => picked() === OTHER || choices().length === 0;
  const ref = (): T3ProjectRef | undefined => choices().find((entry) => entry.id === (picked() || choices()[0]?.id));
  const repositoryPath = () => (manual() ? path().trim() : (ref()?.workspaceRoot ?? ""));
  const address = () => {
    const raw = baseUrl().trim();
    return raw && !/^https?:\/\//.test(raw) ? `http://${raw}` : raw;
  };
  const ready = () => Boolean(repositoryPath() && address());

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready() || busy()) return;
    setBusy(true);
    const created = await store.commands.createProject({
      name: manual() || !ref() ? folderName(repositoryPath()) : label(ref()!),
      repositoryPath: repositoryPath(),
      baseUrl: address(),
      t3ProjectId: manual() ? null : (ref()?.id ?? null),
    });
    setBusy(false);
    if (created) props.onDone?.();
  };

  return (
    <form class="form" onSubmit={submit}>
      <Show when={choices().length > 0}>
        <select
          class="input"
          aria-label="Projekt"
          value={picked() || choices()[0]?.id}
          onChange={(event) => setPicked(event.currentTarget.value)}
        >
          <For each={choices()}>{(entry) => <option value={entry.id}>{label(entry)}</option>}</For>
          <option value={OTHER}>Anderer Ordner …</option>
        </select>
      </Show>
      <Show when={manual()}>
        <input
          class="input"
          aria-label="Ordner des Projekts"
          placeholder="/home/du/projekte/app"
          value={path()}
          onInput={(event) => setPath(event.currentTarget.value)}
        />
      </Show>
      <input
        class="input"
        aria-label="Adresse im Browser"
        placeholder="localhost:3000"
        inputmode="url"
        value={baseUrl()}
        onInput={(event) => setBaseUrl(event.currentTarget.value)}
      />
      <div class="form-foot">
        <Show when={props.onCancel}>
          <Button size="sm" variant="ghost" onClick={props.onCancel}>
            Abbrechen
          </Button>
        </Show>
        <Button type="submit" size="sm" variant="solid" disabled={!ready() || busy()}>
          Anlegen
        </Button>
      </div>
    </form>
  );
}

function ProjectRow(props: { project: Project; store: AppStore }) {
  const store = props.store;
  const project = () => props.project;
  const active = () => store.status()?.activeProjectId === project().id;
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createSignal(project().name);
  const [baseUrl, setBaseUrl] = createSignal(project().baseUrl);
  const dirty = () => name().trim() !== project().name || baseUrl().trim() !== project().baseUrl;

  return (
    <li class="session" classList={{ "is-open": open() }}>
      <div class="session-line">
        <button type="button" class="session-main" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <span class="glyph" classList={{ "glyph-ok": active(), "glyph-muted": !active() }} />
          <span class="session-name">{project().name}</span>
          <span class="session-state">{address(project().baseUrl)}</span>
        </button>
        <Show when={!active()}>
          <Button size="sm" onClick={() => store.commands.setActive({ projectId: project().id })}>
            Wechseln
          </Button>
        </Show>
      </div>
      <Show when={open()}>
        <div class="session-detail">
          <div class="form">
            <input class="input" aria-label="Name" value={name()} onInput={(event) => setName(event.currentTarget.value)} />
            <input
              class="input"
              aria-label="Adresse im Browser"
              value={baseUrl()}
              onInput={(event) => setBaseUrl(event.currentTarget.value)}
            />
          </div>
          <p class="task-facts">
            <span>{project().repositoryPath}</span>
          </p>
          <div class="task-actions">
            <Button
              size="sm"
              disabled={!dirty() || !name().trim() || !baseUrl().trim()}
              onClick={() => store.commands.patchProject(project().id, { name: name().trim(), baseUrl: baseUrl().trim() })}
            >
              Sichern
            </Button>
            <Button size="sm" variant="ghost" onClick={() => store.commands.removeProject(project().id)}>
              <Icon name="trash" size={14} /> Entfernen
            </Button>
          </div>
        </div>
      </Show>
    </li>
  );
}

export function ProjectSettings(props: { store: AppStore }) {
  const store = props.store;
  const [adding, setAdding] = createSignal(false);
  const projects = () => store.snapshot()?.projects ?? [];

  return (
    <section class="group">
      <Show when={projects().length > 0}>
        <ul class="sessions">
          <For each={projects()}>{(project) => <ProjectRow project={project} store={store} />}</For>
        </ul>
      </Show>
      <Show
        when={adding() || projects().length === 0}
        fallback={
          <div class="group-foot">
            <Button size="sm" variant="ghost" onClick={() => setAdding(true)}>
              <Icon name="plus" size={14} /> Projekt hinzufügen
            </Button>
          </div>
        }
      >
        <div class="card">
          <ProjectForm
            store={store}
            onDone={() => setAdding(false)}
            onCancel={projects().length > 0 ? () => setAdding(false) : undefined}
          />
        </div>
      </Show>
    </section>
  );
}
