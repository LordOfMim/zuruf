import type { AgentProfile, Settings as SettingsData, WorkspaceStrategy } from "@zuruf/protocol";
import { For, type JSX, Show, createMemo, createSignal, onMount } from "solid-js";
import { Button, Icon } from "../components/basics";
import type { AppStore } from "../lib/store";
import { plain } from "../lib/store";
import { strategyLabel } from "../lib/text";
import { Shortcuts } from "./Shortcuts";

type Tab = "shortcuts" | "agents" | "speech" | "capture";

const TABS: { id: Tab; label: string }[] = [
  { id: "shortcuts", label: "Kurzbefehle" },
  { id: "agents", label: "Agenten" },
  { id: "speech", label: "Diktat" },
  { id: "capture", label: "Aufnahme" },
];

function Field(props: { label: string; hint?: string; children: JSX.Element }) {
  return (
    <label class="row field">
      <span class="row-label">
        {props.label}
        <Show when={props.hint}>
          <small>{props.hint}</small>
        </Show>
      </span>
      {props.children}
    </label>
  );
}

function Segmented<T extends string>(props: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <span class="segmented" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(option) => (
          <button
            type="button"
            role="radio"
            aria-checked={props.value === option.value}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        )}
      </For>
    </span>
  );
}

function Toggle(props: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      class="switch"
      role="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      onClick={() => props.onChange(!props.checked)}
    >
      <span class="switch-knob" />
    </button>
  );
}

/** Saves one changed slice of the settings immediately; there is no save button. */
function useSettings(store: AppStore) {
  return <K extends keyof SettingsData>(key: K, patch: Partial<SettingsData[K]>) => {
    const current = store.settings();
    if (!current) return;
    const next = plain(current) as SettingsData;
    next[key] = { ...(next[key] as object), ...patch } as SettingsData[K];
    void store.commands.saveSettings(next);
  };
}

function SpeechSettings(props: { store: AppStore }) {
  const update = useSettings(props.store);
  const speech = () => props.store.settings()!.speech;
  return (
    <section class="group">
      <ul class="rows">
        <Field label="Diktat">
          <Toggle checked={speech().enabled} label="Diktat" onChange={(enabled) => update("speech", { enabled })} />
        </Field>
        <Field label="Aufnahme" hint={speech().backend === "daemon" ? "kurze Lücke bei jedem Wechsel" : undefined}>
          <Segmented
            label="Aufnahmeart"
            value={speech().backend}
            options={[
              { value: "continuous", label: "Durchgehend" },
              { value: "daemon", label: "Über Voxtype" },
            ]}
            onChange={(backend) => update("speech", { backend })}
          />
        </Field>
        <Field label="Gesprochenes gehört zur Aufnahme">
          <Segmented
            label="Zuordnung"
            value={speech().attach}
            options={[
              { value: "after", label: "danach" },
              { value: "before", label: "davor" },
            ]}
            onChange={(attach) => update("speech", { attach })}
          />
        </Field>
        <Field label="Sprache">
          <input
            class="input input-short"
            value={speech().language ?? ""}
            placeholder="auto"
            onChange={(event) => update("speech", { language: event.currentTarget.value.trim() || null })}
          />
        </Field>
        <Field label="Modell">
          <input
            class="input"
            value={speech().model ?? ""}
            placeholder="wie in Voxtype"
            onChange={(event) => update("speech", { model: event.currentTarget.value.trim() || null })}
          />
        </Field>
        <Field label="Tonaufnahmen behalten">
          <Toggle
            checked={speech().keepAudio}
            label="Tonaufnahmen behalten"
            onChange={(keepAudio) => update("speech", { keepAudio })}
          />
        </Field>
      </ul>
    </section>
  );
}

function CaptureSettings(props: { store: AppStore }) {
  const update = useSettings(props.store);
  const capture = () => props.store.settings()!.capture;
  const redaction = () => props.store.settings()!.redaction;
  return (
    <section class="group">
      <ul class="rows">
        <Field label="Markierung">
          <span class="inline">
            <input
              type="color"
              class="swatch"
              aria-label="Farbe der Markierung"
              value={capture().ringColor}
              onChange={(event) => update("capture", { ringColor: event.currentTarget.value })}
            />
            <input
              type="range"
              min="12"
              max="64"
              aria-label="Größe der Markierung"
              value={capture().ringRadius}
              onChange={(event) => update("capture", { ringRadius: Number(event.currentTarget.value) })}
            />
          </span>
        </Field>
        <Field label="Jede Taste">
          <Segmented
            label="Verhalten der Aufnahmetasten"
            value={capture().mode}
            options={[
              { value: "new_entry", label: "neuer Eintrag" },
              { value: "extend_open_entry", label: "erweitert den letzten" },
            ]}
            onChange={(mode) => update("capture", { mode })}
          />
        </Field>
        <Field label="Unmarkiertes Bild zusätzlich speichern">
          <Toggle
            checked={capture().keepRawScreenshot}
            label="Unmarkiertes Bild zusätzlich speichern"
            onChange={(keepRawScreenshot) => update("capture", { keepRawScreenshot })}
          />
        </Field>
        <Field label="Adresszeilen-Parameter weglassen">
          <Toggle
            checked={redaction().stripQuery}
            label="Adresszeilen-Parameter weglassen"
            onChange={(stripQuery) => update("redaction", { stripQuery })}
          />
        </Field>
      </ul>
    </section>
  );
}

const STRATEGIES: WorkspaceStrategy[] = ["new_worktree", "existing_worktree", "shared_root"];

function ProfileEditor(props: { store: AppStore; profile: AgentProfile; onClose: () => void }) {
  const store = props.store;
  const [draft, setDraft] = createSignal<AgentProfile>(plain(props.profile));
  const patch = (change: Partial<AgentProfile>) => setDraft({ ...draft(), ...change });

  const providers = createMemo(() => store.catalog()?.providers.filter((provider) => provider.enabled) ?? []);
  const provider = createMemo(() => providers().find((entry) => entry.instanceId === draft().providerInstanceId));
  const model = createMemo(() => provider()?.models.find((entry) => entry.id === draft().model));

  const save = async () => {
    const { id, ...rest } = draft();
    await store.commands.saveProfile(id ? draft() : rest);
    props.onClose();
  };

  return (
    <ul class="rows editor">
      <Field label="Name">
        <input class="input" value={draft().name} onInput={(event) => patch({ name: event.currentTarget.value })} />
      </Field>
      <Field label="Anbieter">
        <select
          class="input"
          value={draft().providerInstanceId}
          onChange={(event) => {
            const next = providers().find((entry) => entry.instanceId === event.currentTarget.value);
            patch({ providerInstanceId: event.currentTarget.value, model: next?.models[0]?.id ?? "", options: {} });
          }}
        >
          <For each={providers()}>{(entry) => <option value={entry.instanceId}>{entry.displayName}</option>}</For>
        </select>
      </Field>
      <Field label="Modell">
        <select
          class="input"
          value={draft().model}
          onChange={(event) => patch({ model: event.currentTarget.value, options: {} })}
        >
          <For each={provider()?.models}>{(entry) => <option value={entry.id}>{entry.label ?? entry.id}</option>}</For>
          <Show when={!model() && draft().model}>
            <option value={draft().model}>{draft().model}</option>
          </Show>
        </select>
      </Field>
      <For each={model()?.options}>
        {(option) => (
          <Field label={option.id}>
            <select
              class="input"
              value={String(draft().options[option.id] ?? option.default ?? "")}
              onChange={(event) => {
                const raw = event.currentTarget.value;
                const value = raw === "true" ? true : raw === "false" ? false : raw;
                patch({ options: { ...draft().options, [option.id]: value } });
              }}
            >
              <For each={option.values}>{(value) => <option value={String(value)}>{String(value)}</option>}</For>
            </select>
          </Field>
        )}
      </For>
      <Field label="Arbeitet in">
        <Segmented
          label="Arbeitsverzeichnis"
          value={draft().workspaceStrategy}
          options={STRATEGIES.map((value) => ({ value, label: strategyLabel[value] }))}
          onChange={(workspaceStrategy) => patch({ workspaceStrategy })}
        />
      </Field>
      <Show when={draft().workspaceStrategy === "existing_worktree"}>
        <Field label="Pfad">
          <input
            class="input"
            value={draft().worktreePath ?? ""}
            onInput={(event) => patch({ worktreePath: event.currentTarget.value })}
          />
        </Field>
      </Show>
      <Field label="Schnellwahl">
        <Segmented
          label="Schnellwahl"
          value={String(draft().slot ?? 0)}
          options={[
            { value: "0", label: "–" },
            { value: "1", label: "1" },
            { value: "2", label: "2" },
            { value: "3", label: "3" },
          ]}
          onChange={(slot) => patch({ slot: Number(slot) || null })}
        />
      </Field>
      <li class="editor-foot">
        <Show when={draft().id}>
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              await store.commands.removeProfile(draft().id);
              props.onClose();
            }}
          >
            <Icon name="trash" size={14} /> Löschen
          </Button>
        </Show>
        <span class="spacer" />
        <Button size="sm" variant="ghost" onClick={props.onClose}>
          Abbrechen
        </Button>
        <Button size="sm" variant="solid" disabled={!draft().name.trim() || !draft().model} onClick={save}>
          Sichern
        </Button>
      </li>
    </ul>
  );
}

function AgentSettings(props: { store: AppStore }) {
  const store = props.store;
  const [editing, setEditing] = createSignal<AgentProfile | null>(null);
  const t3 = () => store.status()?.t3;
  onMount(() => void store.commands.loadCatalog());

  const blank = (): AgentProfile => {
    const first = store.catalog()?.providers.find((provider) => provider.enabled);
    return {
      id: "",
      name: "",
      providerInstanceId: first?.instanceId ?? "",
      model: first?.models[0]?.id ?? "",
      options: {},
      workspaceStrategy: "new_worktree",
    };
  };

  return (
    <>
      <section class="group">
        <ul class="rows">
          <li class="row">
            <span class="row-label">
              T3 Code
              <small>
                {t3()?.state === "connected"
                  ? `verbunden · ${t3()?.version ?? ""}${t3()?.transport === "cli" ? " · eingeschränkt" : ""}`
                  : (t3()?.error ?? "nicht verbunden")}
              </small>
            </span>
            <Button size="sm" onClick={() => store.commands.reconnectT3()}>
              Neu verbinden
            </Button>
          </li>
        </ul>
      </section>

      <section class="group">
        <h2 class="group-title">Profile</h2>
        <Show
          when={!editing()}
          fallback={<ProfileEditor store={store} profile={editing()!} onClose={() => setEditing(null)} />}
        >
          <ul class="rows">
            <For each={store.snapshot()?.profiles}>
              {(profile) => (
                <li class="row">
                  <button type="button" class="row-button" onClick={() => setEditing(profile)}>
                    <span class="row-label">
                      {profile.name}
                      <small>
                        {profile.model} · {strategyLabel[profile.workspaceStrategy]}
                      </small>
                    </span>
                    <Show when={profile.slot}>
                      <span class="row-aside">Schnellwahl {profile.slot}</span>
                    </Show>
                  </button>
                </li>
              )}
            </For>
            <li class="row">
              <button type="button" class="row-button row-add" onClick={() => setEditing(blank())}>
                <Icon name="plus" size={14} /> Profil anlegen
              </button>
            </li>
          </ul>
        </Show>
      </section>
    </>
  );
}

export function Settings(props: { store: AppStore }) {
  const [tab, setTab] = createSignal<Tab>("shortcuts");
  return (
    <div class="page">
      <header class="page-head">
        <h1>Einstellungen</h1>
        <nav class="tabs" aria-label="Bereiche">
          <For each={TABS}>
            {(entry) => (
              <button type="button" aria-current={tab() === entry.id ? "page" : undefined} onClick={() => setTab(entry.id)}>
                {entry.label}
              </button>
            )}
          </For>
        </nav>
      </header>
      <Show when={props.store.settings()}>
        <Show when={tab() === "shortcuts"}>
          <Shortcuts store={props.store} />
        </Show>
        <Show when={tab() === "agents"}>
          <AgentSettings store={props.store} />
        </Show>
        <Show when={tab() === "speech"}>
          <SpeechSettings store={props.store} />
        </Show>
        <Show when={tab() === "capture"}>
          <CaptureSettings store={props.store} />
        </Show>
      </Show>
    </div>
  );
}
