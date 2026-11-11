import type { BrowserSession, SessionMode } from "@zuruf/protocol";
import { For, Show, createSignal } from "solid-js";
import { Button, Icon } from "../components/basics";
import type { AppStore } from "../lib/store";
import { plain } from "../lib/store";
import { sessionModeLabel, shortUrl } from "../lib/text";
import { Segmented } from "./Settings";

function controlLine(session: BrowserSession): string {
  if (session.state !== "running") return session.state === "crashed" ? "abgestürzt" : "aus";
  if (session.control.owner === "agent") return `${session.control.agentId ?? "Agent"} steuert`;
  return session.tabs.find((tab) => tab.active)?.title ?? "läuft";
}

function SessionRow(props: { session: BrowserSession; store: AppStore }) {
  const commands = props.store.commands;
  const session = () => props.session;
  const running = () => session().state === "running" || session().state === "starting";
  const active = () => props.store.status()?.activeSessionId === session().id;
  const [open, setOpen] = createSignal(false);

  return (
    <li class="session" classList={{ "is-open": open(), "is-active": active() }}>
      <div class="session-line">
        <button type="button" class="session-main" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <span class="glyph" classList={{ "glyph-ok": running(), "glyph-muted": !running() }} />
          <span class="session-name">{session().name}</span>
          <span class="tag">{sessionModeLabel[session().mode]}</span>
          <span class="session-state">{controlLine(session())}</span>
        </button>
        <Show when={session().control.owner === "agent"}>
          <Button size="sm" onClick={() => commands.control(session().id, { action: "takeover" })}>
            <Icon name="hand" size={14} /> Übernehmen
          </Button>
        </Show>
        <Button size="sm" onClick={() => commands.sessionCommand(session().id, running() ? "stop" : "start")}>
          {running() ? "Schließen" : "Öffnen"}
        </Button>
      </div>

      <Show when={open()}>
        <div class="session-detail">
          <Show when={session().tabs.length > 0}>
            <ul class="tabs-list">
              <For each={session().tabs}>
                {(tab) => (
                  <li classList={{ "is-current": tab.active }}>
                    <span>{tab.title || shortUrl(tab.url)}</span>
                    <small>{shortUrl(tab.url)}</small>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <label class="check">
            <input
              type="checkbox"
              checked={session().agentAccess}
              onChange={(event) =>
                props.store.commands.control(session().id, {
                  action: event.currentTarget.checked ? "grant" : "revoke",
                })
              }
            />
            Agenten dürfen diese Sitzung steuern
          </label>
          <div class="task-actions">
            <Show when={!active()}>
              <Button size="sm" onClick={() => commands.setActive({ sessionId: session().id })}>
                Für Review verwenden
              </Button>
            </Show>
            <Button size="sm" variant="ghost" onClick={() => commands.sessionCommand(session().id, "duplicate")}>
              Duplizieren
            </Button>
            <Button size="sm" variant="ghost" onClick={() => commands.removeSession(session().id)}>
              <Icon name="trash" size={14} /> Löschen
            </Button>
          </div>
        </div>
      </Show>
    </li>
  );
}

export function Sessions(props: { store: AppStore }) {
  const store = props.store;
  const [name, setName] = createSignal("");
  const [mode, setMode] = createSignal<SessionMode>("isolated");

  const create = async (event: SubmitEvent) => {
    event.preventDefault();
    const projectId = store.status()?.activeProjectId;
    if (!projectId || !name().trim()) return;
    await store.commands.createSession({ projectId, name: name().trim(), mode: mode(), agentAccess: mode() === "shared" });
    setName("");
  };

  return (
    <div class="page">
      <header class="page-head">
        <h1>Sitzungen</h1>
        <Show when={store.settings()}>
          {(settings) => (
            <Segmented
              label="Fenster"
              value={settings().ui.browser ?? "integrated"}
              options={[
                { value: "integrated", label: "Ein Fenster" },
                { value: "separate", label: "Getrennt" },
              ]}
              onChange={(browser) => {
                const next = plain(settings());
                void store.commands.saveSettings({ ...next, ui: { ...next.ui, browser } });
              }}
            />
          )}
        </Show>
      </header>
      <section class="group">
        <ul class="sessions">
          <For each={store.snapshot()?.sessions}>{(session) => <SessionRow session={session} store={store} />}</For>
        </ul>
        <form class="create" onSubmit={create}>
          <input
            class="input"
            placeholder="Neue Sitzung"
            aria-label="Name der neuen Sitzung"
            value={name()}
            onInput={(event) => setName(event.currentTarget.value)}
          />
          <select class="input input-short" aria-label="Art" value={mode()} onChange={(event) => setMode(event.currentTarget.value as SessionMode)}>
            <option value="shared">{sessionModeLabel.shared}</option>
            <option value="isolated">{sessionModeLabel.isolated}</option>
          </select>
          <Button type="submit" disabled={!name().trim()}>
            Anlegen
          </Button>
        </form>
      </section>
    </div>
  );
}
