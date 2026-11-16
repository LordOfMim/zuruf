import type { BrowserOptions, Project, T3ProjectRef } from "@zuruf/protocol";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { BrowserOptionFields, sameBrowserOptions } from "../components/BrowserOptions";
import { Button, Icon } from "../components/basics";
import { bareAddress, folderName, withScheme } from "../lib/address";
import type { AppStore } from "../lib/store";

const OTHER = "__other";
const GENERAL = "__general";

/**
 * Add a project. Three kinds: one T3 already knows, any other folder, or a general project
 * without a folder that only collects start options (things opened all the time, prototypes).
 */
export function ProjectForm(props: { store: AppStore; onDone?: () => void; onCancel?: () => void }) {
  const store = props.store;
  const connected = () => store.status()?.t3.state === "connected";
  const [refs] = createResource(connected, async (ok) => (ok ? ((await store.commands.t3Projects()) ?? []) : []));

  const known = createMemo(() => new Set(store.snapshot()?.projects.map((project) => project.repositoryPath)));
  const choices = createMemo(() => (refs() ?? []).filter((ref) => !known().has(ref.workspaceRoot)));

  const [picked, setPicked] = createSignal<string>("");
  const [name, setName] = createSignal("");
  const [path, setPath] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  // T3 titles repeat ("main" for every checkout), so ambiguous ones are named by their last two folders.
  const label = (entry: T3ProjectRef) => {
    const unique = choices().filter((other) => other.title === entry.title).length === 1;
    return unique ? entry.title : entry.workspaceRoot.replace(/\/+$/, "").split("/").slice(-2).join("/");
  };

  const kind = () => picked() || choices()[0]?.id || OTHER;
  const general = () => kind() === GENERAL;
  const manual = () => kind() === OTHER;
  const ref = (): T3ProjectRef | undefined => choices().find((entry) => entry.id === kind());
  const repositoryPath = () => (manual() ? path().trim() : (ref()?.workspaceRoot ?? ""));
  const ready = () => (general() ? Boolean(name().trim()) : Boolean(repositoryPath()));

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready() || busy()) return;
    setBusy(true);
    const fromFolder = ref() ? label(ref()!) : folderName(repositoryPath());
    const created = await store.commands.createProject({
      name: general() ? name().trim() : fromFolder,
      repositoryPath: general() ? "" : repositoryPath(),
      baseUrl: withScheme(baseUrl()),
      t3ProjectId: ref()?.id ?? null,
    });
    setBusy(false);
    if (created) props.onDone?.();
  };

  return (
    <form class="form" onSubmit={submit}>
      <select class="input" aria-label="Art des Projekts" value={kind()} onChange={(event) => setPicked(event.currentTarget.value)}>
        <For each={choices()}>{(entry) => <option value={entry.id}>{label(entry)}</option>}</For>
        <option value={OTHER}>Anderer Ordner …</option>
        <option value={GENERAL}>Allgemeines Projekt (ohne Ordner)</option>
      </select>
      <Show when={general()}>
        <input
          class="input"
          aria-label="Name des Projekts"
          placeholder="Allgemein"
          value={name()}
          onInput={(event) => setName(event.currentTarget.value)}
        />
        <p class="form-note">Sammelt Startoptionen, die zu keinem einzelnen Ordner gehören. Jede kann ihren eigenen Ordner mitbringen.</p>
      </Show>
      <Show when={manual()}>
        <input
          class="input input-mono"
          aria-label="Ordner des Projekts"
          placeholder="/home/du/projekte/app"
          spellcheck={false}
          value={path()}
          onInput={(event) => setPath(event.currentTarget.value)}
        />
      </Show>
      <Show when={!general()}>
        <input
          class="input"
          aria-label="Adresse im Browser (optional)"
          placeholder="localhost:3000 (optional)"
          inputmode="url"
          value={baseUrl()}
          onInput={(event) => setBaseUrl(event.currentTarget.value)}
        />
      </Show>
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
  const [folder, setFolder] = createSignal(project().repositoryPath);
  const [browser, setBrowser] = createSignal<BrowserOptions>({ ...project().browser });

  const dirty = () =>
    name().trim() !== project().name ||
    withScheme(baseUrl()) !== project().baseUrl ||
    folder().trim() !== project().repositoryPath ||
    !sameBrowserOptions(browser(), project().browser);
  const startOptions = () =>
    (store.snapshot()?.launchers ?? []).filter((launcher) => launcher.projectId === project().id).length;
  const summary = () => {
    const parts = [project().repositoryPath ? folderName(project().repositoryPath) : "Allgemein"];
    if (project().baseUrl) parts.push(bareAddress(project().baseUrl));
    if (startOptions() > 0) parts.push(startOptions() === 1 ? "1 Startoption" : `${startOptions()} Startoptionen`);
    return parts.join(" · ");
  };

  const save = () =>
    store.commands.patchProject(project().id, {
      name: name().trim(),
      baseUrl: withScheme(baseUrl()),
      repositoryPath: folder().trim(),
      browser: browser(),
    });

  return (
    <li class="session" classList={{ "is-open": open() }}>
      <div class="session-line">
        <button type="button" class="session-main" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <span class="glyph" classList={{ "glyph-ok": active(), "glyph-muted": !active() }} />
          <span class="session-name">{project().name}</span>
          <span class="session-state">{summary()}</span>
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
            <div class="fields fields-2">
              <label class="labeled">
                <span>Name</span>
                <input class="input" value={name()} onInput={(event) => setName(event.currentTarget.value)} />
              </label>
              <label class="labeled">
                <span>
                  Adresse im Browser <small>optional</small>
                </span>
                <input class="input" inputmode="url" value={baseUrl()} onInput={(event) => setBaseUrl(event.currentTarget.value)} />
              </label>
            </div>
            <label class="labeled">
              <span>
                Ordner <small>optional</small>
              </span>
              <input
                class="input input-mono"
                placeholder="kein Ordner – allgemeines Projekt"
                spellcheck={false}
                value={folder()}
                onInput={(event) => setFolder(event.currentTarget.value)}
              />
            </label>
            <BrowserOptionFields value={browser()} inheritLabel="wie Einstellungen" onChange={setBrowser} />
          </div>
          <div class="task-actions">
            <Button size="sm" disabled={!dirty() || !name().trim()} onClick={save}>
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
