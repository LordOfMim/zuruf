import { For, Show, createMemo, createSignal } from "solid-js";
import { Button, Icon, Keys, Shot } from "../components/basics";
import { Entry } from "../components/Entry";
import { Menu } from "../components/Menu";
import { Recorder } from "../components/Recorder";
import { TaskRow } from "../components/TaskRow";
import type { AppStore } from "../lib/store";
import { strategyLabel } from "../lib/text";

/** Who drives the shared browser. Clicking while an agent drives takes control back. */
function ControlChip(props: { store: AppStore }) {
  const session = () => props.store.activeSession();
  const control = () => session()?.control;
  const agentDrives = () => control()?.owner === "agent";
  return (
    <Show when={session()}>
      <button
        type="button"
        class="chip"
        classList={{ "chip-agent": agentDrives(), "chip-settling": control()?.settling }}
        disabled={!agentDrives()}
        title={agentDrives() ? "Steuerung übernehmen" : undefined}
        onClick={() => props.store.commands.control(session()!.id, { action: "takeover" })}
      >
        <Show when={agentDrives()} fallback="Du steuerst">
          <Icon name="hand" size={13} />
          {control()?.settling ? "Agent hält an …" : `${control()?.agentId ?? "Agent"} steuert`}
        </Show>
      </button>
    </Show>
  );
}

/** The capture keys as the empty state: the layout teaches the three gestures. */
function EmptyDraft(props: { store: AppStore }) {
  const bindingFor = props.store.bindingFor;
  return (
    <div class="empty">
      <p class="empty-keys">
        <span>
          <Keys chord={bindingFor("capture.viewport")} /> Ansicht
        </span>
        <span>
          <Keys chord={bindingFor("capture.region")} /> Ausschnitt
        </span>
        <span>
          <Keys chord={bindingFor("capture.dom")} /> Element
        </span>
      </p>
      <p class="empty-line">Zeigen, sprechen, weiter.</p>
    </div>
  );
}

function DispatchBar(props: { store: AppStore }) {
  const store = props.store;
  const profile = () => store.draftProfile();
  const count = () => store.draftAnnotations().length;
  const canSend = () => count() > 0 || Boolean(store.draft()?.note.trim());

  return (
    <footer class="dispatch">
      <Menu
        placement="top"
        trigger={(toggle, open) => (
          <button type="button" class="profile" aria-haspopup="menu" aria-expanded={open()} onClick={toggle}>
            <span class="profile-name">{profile()?.name ?? "Kein Profil"}</span>
            <span class="profile-sub">{profile() ? strategyLabel[profile()!.workspaceStrategy] : ""}</span>
            <Icon name="chevron" size={14} />
          </button>
        )}
      >
        {() => (
          <For each={store.snapshot()?.profiles}>
            {(option) => (
              <button
                type="button"
                class="menu-item"
                role="menuitemradio"
                aria-checked={option.id === profile()?.id}
                data-close
                onClick={() => store.draft() && store.commands.patchBatch(store.draft()!.id, { profileId: option.id })}
              >
                <span class="menu-check">
                  <Show when={option.id === profile()?.id}>
                    <Icon name="check" size={14} />
                  </Show>
                </span>
                <span class="menu-label">
                  {option.name}
                  <small>
                    {option.model} · {strategyLabel[option.workspaceStrategy]}
                  </small>
                </span>
                <Show when={option.slot}>
                  <Keys chord={store.bindingFor(`profile.slot${option.slot}` as "profile.slot1")} />
                </Show>
              </button>
            )}
          </For>
        )}
      </Menu>
      <Button
        variant="solid"
        disabled={!canSend()}
        onClick={() => store.draft() && store.commands.dispatch(store.draft()!.id)}
      >
        Senden <Keys chord={store.bindingFor("batch.dispatch")} />
      </Button>
    </footer>
  );
}

/** The companion that sits next to the page under review. */
export function Panel(props: { store: AppStore; onOpenTasks?: () => void }) {
  const store = props.store;
  const [lightbox, setLightbox] = createSignal<string | null>(null);
  const reviewOn = () => store.status()?.reviewMode ?? false;

  // Only tasks that are still moving or need the user stay visible here.
  const liveTasks = createMemo(() =>
    store
      .tasks()
      .filter((task) => !task.markedDone && task.status !== "cancelled")
      .slice(0, 4),
  );

  return (
    <div class="panel" classList={{ "is-paused": !reviewOn() }}>
      <header class="panel-head">
        <button
          type="button"
          class="switch"
          role="switch"
          aria-checked={reviewOn()}
          aria-label="Review-Modus"
          onClick={() => store.commands.setReviewMode(!reviewOn())}
        >
          <span class="switch-knob" />
        </button>
        <span class="panel-project">{store.activeProject()?.name ?? "Zuruf"}</span>
        <ControlChip store={store} />
      </header>

      <Recorder store={store} />

      <section class="panel-scroll">
        <Show when={store.draftAnnotations().length > 0} fallback={<EmptyDraft store={store} />}>
          <ul class="entries">
            <For each={store.draftAnnotations()}>
              {(annotation) => <Entry annotation={annotation} store={store} onOpenShot={setLightbox} />}
            </For>
          </ul>
        </Show>
      </section>

      <DispatchBar store={store} />

      <Show when={liveTasks().length > 0}>
        <section class="panel-tasks" aria-label="Aufgaben">
          <ul class="tasks">
            <For each={liveTasks()}>{(task) => <TaskRow task={task} store={store} compact />}</For>
          </ul>
        </section>
      </Show>

      <Show when={lightbox()}>
        {(url) => (
          <button type="button" class="lightbox" aria-label="Schließen" onClick={() => setLightbox(null)}>
            <Shot url={url()} resolve={store.asset} alt="Aufnahme" />
          </button>
        )}
      </Show>
    </div>
  );
}
