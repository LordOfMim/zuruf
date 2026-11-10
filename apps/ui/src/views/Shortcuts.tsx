import type { ActionId, Binding, BindingIssue, Chord } from "@zuruf/protocol";
import { For, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { Button, Keys } from "../components/basics";
import { chordFromEvent, defaultBindings, sameChord } from "../lib/chord";
import type { AppStore } from "../lib/store";
import { actionLabel } from "../lib/text";

type Scope = Binding["scope"];

const GROUPS: { title: string; scope: Scope; actions: ActionId[] }[] = [
  {
    title: "Im Review-Browser",
    scope: "page",
    actions: [
      "capture.viewport",
      "capture.region",
      "capture.dom",
      "comment.text",
      "speech.toggle",
      "batch.dispatch",
      "batch.send_to_thread",
      "action.cancel",
      "panel.toggle",
      "profile.slot1",
      "profile.slot2",
      "profile.slot3",
    ],
  },
  {
    title: "Überall am Rechner",
    scope: "global",
    actions: ["review.toggle", "control.takeover", "app.open", "batch.dispatch", "speech.toggle"],
  },
];

const ISSUE_TEXT: Record<BindingIssue["kind"], (detail?: string | null) => string> = {
  duplicate: (detail) => `Schon belegt: ${actionLabel[detail as ActionId] ?? detail ?? "andere Aktion"}`,
  needs_modifier: () => "Braucht Strg, Alt, Umschalt oder Super",
  taken_by_system: (detail) => `Vom System belegt${detail ? `: ${detail}` : ""}`,
  unsupported_key: () => "Diese Taste geht hier nicht",
};

/**
 * Rebinding: click a keycap, press the new chord. The change is validated
 * locally at once and by the daemon (system conflicts) before it is saved.
 */
export function Shortcuts(props: { store: AppStore }) {
  const store = props.store;
  const [recording, setRecording] = createSignal<{ action: ActionId; scope: Scope } | null>(null);
  const [issue, setIssue] = createSignal<(BindingIssue & { chord: Chord }) | null>(null);

  const bindings = createMemo(() => store.settings()?.bindings ?? []);
  const chordOf = (action: ActionId, scope: Scope) =>
    bindings().find((binding) => binding.action === action && binding.scope === scope)?.chord ?? null;
  const isRecording = (action: ActionId, scope: Scope) =>
    recording()?.action === action && recording()?.scope === scope;

  const stop = () => {
    setRecording(null);
    window.removeEventListener("keydown", onKey, true);
  };
  onCleanup(stop);

  async function assign(action: ActionId, scope: Scope, chord: Chord | null) {
    const settings = store.settings();
    if (!settings) return;

    if (chord) {
      if (scope === "global" && chord.mods.length === 0) {
        setIssue({ action, scope, kind: "needs_modifier", chord });
        return;
      }
      const clash = bindings().find(
        (binding) => binding.scope === scope && binding.action !== action && sameChord(binding.chord, chord),
      );
      if (clash) {
        setIssue({ action, scope, kind: "duplicate", detail: clash.action, chord });
        return;
      }
    }

    const exists = bindings().some((binding) => binding.action === action && binding.scope === scope);
    const next: Binding[] = exists
      ? bindings().map((binding) =>
          binding.action === action && binding.scope === scope ? { ...binding, chord } : { ...binding },
        )
      : [...bindings().map((binding) => ({ ...binding })), { action, scope, chord }];

    const remote = await store.commands.checkBindings(next);
    const blocking = remote?.find((entry) => entry.action === action && entry.scope === scope);
    if (blocking && chord) {
      setIssue({ ...blocking, chord });
      return;
    }
    setIssue(null);
    await store.commands.saveSettings({ ...structuredClone(settings), bindings: next });
  }

  function onKey(event: KeyboardEvent) {
    const target = recording();
    if (!target) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.code === "Escape" && !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
      // Escape alone leaves recording, except when rebinding "cancel" itself.
      if (target.action !== "action.cancel") {
        stop();
        return;
      }
    }
    const chord = chordFromEvent(event);
    if (!chord) return;
    stop();
    void assign(target.action, target.scope, chord);
  }

  const start = (action: ActionId, scope: Scope) => {
    setIssue(null);
    setRecording({ action, scope });
    window.addEventListener("keydown", onKey, true);
  };

  const reset = async () => {
    const settings = store.settings();
    if (!settings) return;
    setIssue(null);
    await store.commands.saveSettings({ ...structuredClone(settings), bindings: defaultBindings() });
  };

  return (
    <div class="shortcuts">
      <For each={GROUPS}>
        {(group) => (
          <section class="group">
            <h2 class="group-title">{group.title}</h2>
            <ul class="rows">
              <For each={group.actions}>
                {(action) => {
                  const chord = () => chordOf(action, group.scope);
                  const problem = () => {
                    const current = issue();
                    return current && current.action === action && current.scope === group.scope ? current : null;
                  };
                  return (
                    <li class="row" classList={{ "has-issue": Boolean(problem()) }}>
                      <span class="row-label">{actionLabel[action]}</span>
                      <Show when={problem()}>
                        {(current) => (
                          <span class="row-issue" role="alert">
                            <Keys chord={current().chord} /> {ISSUE_TEXT[current().kind](current().detail)}
                          </span>
                        )}
                      </Show>
                      <Show when={chord() && !isRecording(action, group.scope)}>
                        <button
                          type="button"
                          class="row-clear"
                          aria-label={`Belegung für ${actionLabel[action]} entfernen`}
                          onClick={() => assign(action, group.scope, null)}
                        >
                          Entfernen
                        </button>
                      </Show>
                      <button
                        type="button"
                        class="keyslot"
                        classList={{ "is-recording": isRecording(action, group.scope), "is-empty": !chord() }}
                        aria-label={`Belegung für ${actionLabel[action]} ändern`}
                        onClick={() => (isRecording(action, group.scope) ? stop() : start(action, group.scope))}
                      >
                        <Show when={!isRecording(action, group.scope)} fallback="Taste drücken …">
                          <Show when={chord()} fallback="Belegen">
                            <Keys chord={chord()} />
                          </Show>
                        </Show>
                      </button>
                    </li>
                  );
                }}
              </For>
            </ul>
          </section>
        )}
      </For>
      <div class="group-foot">
        <Button variant="ghost" size="sm" onClick={reset}>
          Standard wiederherstellen
        </Button>
      </div>
    </div>
  );
}
