import type { T3Catalog } from "@zuruf/protocol";
import { For, type JSX, Match, Show, Switch, createEffect, createMemo, createSignal } from "solid-js";
import { Button } from "../components/basics";
import type { AppStore } from "../lib/store";
import { ProjectForm } from "./Projects";

type StepState = "done" | "now" | "later";

function Step(props: { state: StepState; title: string; value?: string; children: JSX.Element }) {
  return (
    <li class="step" classList={{ "is-now": props.state === "now", "is-later": props.state === "later" }}>
      <span
        class="glyph"
        classList={{ "glyph-ok": props.state === "done", "glyph-muted": props.state !== "done" }}
        aria-hidden="true"
      />
      <div class="step-body">
        <p class="step-title">
          {props.title}
          <Show when={props.state === "done" && props.value}>
            <span class="step-value">{props.value}</span>
          </Show>
        </p>
        <Show when={props.state === "now"}>{props.children}</Show>
      </div>
    </li>
  );
}

/** Pick provider and model once; everything else about a profile can wait for the settings. */
function FirstAgent(props: { store: AppStore; catalog: T3Catalog | null }) {
  const providers = createMemo(() => props.catalog?.providers.filter((entry) => entry.enabled) ?? []);
  const [providerId, setProviderId] = createSignal("");
  const [modelId, setModelId] = createSignal("");
  const provider = () => providers().find((entry) => entry.instanceId === providerId()) ?? providers()[0];
  const model = () => provider()?.models.find((entry) => entry.id === modelId()) ?? provider()?.models[0];

  const save = () => {
    const chosen = provider();
    const chosenModel = model();
    if (!chosen || !chosenModel) return;
    return props.store.commands.saveProfile({
      name: chosen.displayName,
      providerInstanceId: chosen.instanceId,
      model: chosenModel.id,
      options: {},
      workspaceStrategy: "new_worktree",
      slot: 1,
    });
  };

  return (
    <div class="form">
      <select
        class="input"
        aria-label="Anbieter"
        value={provider()?.instanceId}
        onChange={(event) => {
          setProviderId(event.currentTarget.value);
          setModelId("");
        }}
      >
        <For each={providers()}>{(entry) => <option value={entry.instanceId}>{entry.displayName}</option>}</For>
      </select>
      <select class="input" aria-label="Modell" value={model()?.id} onChange={(event) => setModelId(event.currentTarget.value)}>
        <For each={provider()?.models}>{(entry) => <option value={entry.id}>{entry.label ?? entry.id}</option>}</For>
      </select>
      <div class="form-foot">
        <Button size="sm" variant="solid" disabled={!model()} onClick={save}>
          Übernehmen
        </Button>
      </div>
    </div>
  );
}

/** First start: three things Zuruf cannot guess. Shown instead of the app until they are settled. */
export function Setup(props: { store: AppStore }) {
  const store = props.store;
  const t3 = () => store.status()?.t3;
  const connected = () => t3()?.state === "connected";
  const project = () => store.activeProject();
  const profile = () => store.snapshot()?.profiles[0];
  const [pairing, setPairing] = createSignal(false);

  createEffect(() => {
    if (connected() && !store.catalog()) void store.commands.loadCatalog();
  });

  const pair = async () => {
    setPairing(true);
    await store.commands.pairT3();
    setPairing(false);
  };

  const state = (index: number): StepState => {
    const done = [connected(), Boolean(project()), Boolean(profile())];
    if (done[index]) return "done";
    return done.slice(0, index).every(Boolean) ? "now" : "later";
  };

  return (
    <div class="setup">
      <span class="mark mark-lg" aria-hidden="true" />
      <h1>Einrichten</h1>
      <ol class="steps">
        <Step state={state(0)} title="T3 Code" value="verbunden">
          <Switch>
            <Match when={t3()?.state === "connecting" || pairing()}>
              <p class="step-note">verbindet …</p>
            </Match>
            <Match when={true}>
              <Show when={t3()?.error}>
                <p class="step-note step-error">{t3()?.error}</p>
              </Show>
              <div class="form-foot">
                <Button size="sm" variant="solid" onClick={pair}>
                  Verbinden
                </Button>
              </div>
            </Match>
          </Switch>
        </Step>
        <Step state={state(1)} title="Projekt" value={project()?.name}>
          <ProjectForm store={store} />
        </Step>
        <Step state={state(2)} title="Agent" value={profile()?.name}>
          <FirstAgent store={store} catalog={store.catalog()} />
        </Step>
      </ol>
    </div>
  );
}
