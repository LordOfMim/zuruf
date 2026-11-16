import type { BrowserOptions, Launcher } from "@zuruf/protocol";
import { For, Show, createSignal } from "solid-js";
import { BrowserOptionFields } from "../components/BrowserOptions";
import { Button, Icon } from "../components/basics";
import { bareAddress, withScheme } from "../lib/address";
import type { AppStore } from "../lib/store";
import { launcherStateLabel } from "../lib/text";

/**
 * Create or change a start option. Only this form writes commands: Zuruf runs them on this
 * machine, so agents get no way to set one.
 */
function LauncherEditor(props: {
  store: AppStore;
  projectId: string;
  launcher?: Launcher;
  onDone: () => void;
  onCancel?: () => void;
}) {
  const store = props.store;
  const initial = props.launcher;
  const [name, setName] = createSignal(initial?.name ?? "");
  const [url, setUrl] = createSignal(initial?.url ?? "");
  const [command, setCommand] = createSignal(initial?.command ?? "");
  const [folder, setFolder] = createSignal(initial?.repositoryPath ?? "");
  const [workingDir, setWorkingDir] = createSignal(initial?.workingDir ?? "");
  const [memory, setMemory] = createSignal(initial?.memoryLimitMb ? String(initial.memoryLimitMb) : "");
  const [browser, setBrowser] = createSignal<BrowserOptions>({ ...(initial?.browser ?? {}) });
  const [busy, setBusy] = createSignal(false);

  const projectFolder = () => store.activeProject()?.repositoryPath ?? "";
  const ready = () => Boolean(name().trim() && url().trim());

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready() || busy()) return;
    setBusy(true);
    const limit = Number(memory());
    const body = {
      name: name().trim(),
      url: withScheme(url()),
      command: command().trim() || null,
      repositoryPath: folder().trim() || null,
      workingDir: workingDir().trim() || null,
      memoryLimitMb: command().trim() && limit > 0 ? limit : null,
      browser: browser(),
    };
    const saved = initial
      ? await store.commands.patchLauncher(initial.id, body)
      : await store.commands.createLauncher({ projectId: props.projectId, ...body });
    setBusy(false);
    if (saved) props.onDone();
  };

  return (
    <form class="form" onSubmit={submit}>
      <div class="fields fields-2">
        <label class="labeled">
          <span>Name</span>
          <input class="input" placeholder="Entwicklungsserver" value={name()} onInput={(event) => setName(event.currentTarget.value)} />
        </label>
        <label class="labeled">
          <span>Adresse</span>
          <input
            class="input"
            placeholder="localhost:3000"
            inputmode="url"
            value={url()}
            onInput={(event) => setUrl(event.currentTarget.value)}
          />
        </label>
      </div>
      <label class="labeled">
        <span>
          Startbefehl <small>optional</small>
        </span>
        <input
          class="input input-mono"
          placeholder="pnpm dev"
          spellcheck={false}
          autocomplete="off"
          value={command()}
          onInput={(event) => setCommand(event.currentTarget.value)}
        />
      </label>
      <Show when={command().trim()}>
        <div class="fields fields-2">
          <label class="labeled">
            <span>
              Befehl ausführen in <small>optional</small>
            </span>
            <input
              class="input input-mono"
              placeholder={folder().trim() || projectFolder() || "/home/du/projekte/app"}
              spellcheck={false}
              value={workingDir()}
              onInput={(event) => setWorkingDir(event.currentTarget.value)}
            />
          </label>
          <label class="labeled">
            <span>
              Speicherlimit in MB <small>optional</small>
            </span>
            <input
              class="input"
              type="number"
              min="64"
              step="64"
              placeholder="ohne Limit"
              value={memory()}
              onInput={(event) => setMemory(event.currentTarget.value)}
            />
          </label>
        </div>
        <p class="form-note">
          Zuruf startet den Befehl, wartet, bis die Adresse antwortet, und beendet ihn beim Stoppen.
        </p>
      </Show>
      <label class="labeled">
        <span>
          Ordner für Agenten <small>optional</small>
        </span>
        <input
          class="input input-mono"
          placeholder={projectFolder() || "/home/du/projekte/app"}
          spellcheck={false}
          value={folder()}
          onInput={(event) => setFolder(event.currentTarget.value)}
        />
      </label>
      <BrowserOptionFields value={browser()} inheritLabel="wie Projekt" onChange={setBrowser} />
      <div class="form-foot">
        <Show when={initial}>
          {(launcher) => (
            <Button size="sm" variant="ghost" onClick={() => store.commands.removeLauncher(launcher().id)}>
              <Icon name="trash" size={14} /> Löschen
            </Button>
          )}
        </Show>
        <span class="spacer" />
        <Show when={props.onCancel}>
          <Button size="sm" variant="ghost" onClick={props.onCancel}>
            Abbrechen
          </Button>
        </Show>
        <Button type="submit" size="sm" variant="solid" disabled={!ready() || busy()}>
          {initial ? "Sichern" : "Anlegen"}
        </Button>
      </div>
    </form>
  );
}

function LauncherRow(props: { launcher: Launcher; store: AppStore }) {
  const store = props.store;
  const launcher = () => props.launcher;
  const [open, setOpen] = createSignal(false);

  const session = () => store.sessions().find((entry) => entry.launcherId === launcher().id);
  const sessionOpen = () => session()?.state === "running" || session()?.state === "starting";
  const process = () => launcher().process.state;
  const commandAlive = () => process() === "running" || process() === "starting";
  const alive = () => commandAlive() || sessionOpen();

  const tone = () => {
    if (process() === "failed") return "glyph-danger";
    if (process() === "starting") return "glyph-busy";
    return alive() ? "glyph-ok" : "glyph-muted";
  };
  const stateLine = () => {
    if (launcher().command) return launcherStateLabel[process()];
    return sessionOpen() ? "offen" : "";
  };

  return (
    <li class="session" classList={{ "is-open": open() }}>
      <div class="session-line">
        <button type="button" class="session-main" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <span class={`glyph ${tone()}`} />
          <span class="session-name">{launcher().name}</span>
          <Show when={launcher().command}>
            <span class="tag">
              <Icon name="terminal" size={12} /> Befehl
            </span>
          </Show>
          <span class="session-state">
            {bareAddress(launcher().url)}
            <Show when={stateLine()}> · {stateLine()}</Show>
          </span>
        </button>
        <Show when={alive()}>
          <Button size="sm" onClick={() => store.commands.launcherCommand(launcher().id, "stop")}>
            <Icon name="stop" size={14} /> Stoppen
          </Button>
        </Show>
        <Show when={process() !== "starting"}>
          <Button size="sm" onClick={() => store.commands.launcherCommand(launcher().id, "start")}>
            <Icon name={alive() ? "external" : "play"} size={14} /> {alive() ? "Zeigen" : "Starten"}
          </Button>
        </Show>
      </div>
      <Show when={launcher().process.error}>
        <p class="session-issue">{launcher().process.error}</p>
      </Show>
      <Show when={open()}>
        <div class="session-detail">
          <LauncherEditor store={store} projectId={launcher().projectId} launcher={launcher()} onDone={() => setOpen(false)} />
        </div>
      </Show>
    </li>
  );
}

/** The start options of the project in front. */
export function Launchers(props: { store: AppStore }) {
  const store = props.store;
  const [adding, setAdding] = createSignal(false);
  const launchers = () => store.launchers();
  const projectId = () => store.status()?.activeProjectId ?? "";

  return (
    <section class="group">
      <h2 class="group-title">Startoptionen</h2>
      <Show when={launchers().length > 0}>
        <ul class="sessions">
          <For each={launchers()}>{(launcher) => <LauncherRow launcher={launcher} store={store} />}</For>
        </ul>
      </Show>
      <Show
        when={adding()}
        fallback={
          <Show
            when={launchers().length > 0}
            fallback={
              <div class="blank">
                <p>Lege fest, was sich in diesem Projekt starten lässt: eine Adresse, auf Wunsch mit Startbefehl.</p>
                <Button size="sm" variant="solid" onClick={() => setAdding(true)}>
                  <Icon name="plus" size={14} /> Startoption anlegen
                </Button>
              </div>
            }
          >
            <div class="group-foot">
              <Button size="sm" variant="ghost" onClick={() => setAdding(true)}>
                <Icon name="plus" size={14} /> Startoption hinzufügen
              </Button>
            </div>
          </Show>
        }
      >
        <div class="card">
          <LauncherEditor store={store} projectId={projectId()} onDone={() => setAdding(false)} onCancel={() => setAdding(false)} />
        </div>
      </Show>
    </section>
  );
}
