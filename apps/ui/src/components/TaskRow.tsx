import type { Task } from "@zuruf/protocol";
import { Match, Show, Switch, createSignal } from "solid-js";
import type { AppStore } from "../lib/store";
import { strategyLabel, taskStatusLabel, timeAgo } from "../lib/text";
import { Button, Icon, StatusGlyph, taskTone } from "./basics";

/** What the row says on the right: the state in words, or who is working. */
function stateLine(task: Task): string {
  if (task.status === "running") return `${task.providerDisplayName ?? "Agent"} arbeitet`;
  if (task.status === "completed" && task.markedDone) return "Erledigt";
  return taskStatusLabel[task.status];
}

/**
 * A dispatched batch. Collapsed: one calm line. Expanded: the agent's last
 * words and exactly the actions that make sense in this state.
 */
const openTasks = new Set<string>();

export function TaskRow(props: { task: Task; store: AppStore; compact?: boolean }) {
  // Rows remount when a task changes group; what the user opened stays open.
  const [open, setOpenSignal] = createSignal(openTasks.has(props.task.id));
  const setOpen = (next: boolean) => {
    if (next) openTasks.add(props.task.id);
    else openTasks.delete(props.task.id);
    setOpenSignal(next);
  };
  const [followUp, setFollowUp] = createSignal("");
  const [adoptId, setAdoptId] = createSignal("");
  const commands = props.store.commands;
  const task = () => props.task;
  const settled = () => task().status === "completed" || task().status === "cancelled";

  const sendFollowUp = async () => {
    const text = followUp().trim();
    if (!text) return;
    setFollowUp("");
    await commands.followUp(task().id, { text });
  };

  return (
    <li
      class={`task tone-${taskTone(task().status)}`}
      classList={{ "is-open": open(), "is-done": task().markedDone, "is-compact": props.compact }}
    >
      <button type="button" class="task-line" aria-expanded={open()} onClick={() => setOpen(!open())}>
        <StatusGlyph status={task().status} />
        <span class="task-title">{task().title}</span>
        <span class="task-state">{stateLine(task())}</span>
      </button>

      <Show when={open()}>
        <div class="task-detail">
          <Switch>
            <Match when={task().status === "needs_approval" || task().status === "needs_review"}>
              <p class="task-callout">
                <Show when={task().pendingRequest} fallback="Der Agent wartet auf dich.">
                  <code>{task().pendingRequest}</code>
                </Show>
              </p>
            </Match>
            <Match when={task().status === "unknown_remote_state"}>
              <p class="task-callout">
                Unklar, ob der Thread in T3 angelegt wurde. Damit nichts doppelt läuft, entscheidest du.
              </p>
              <div class="task-resolve">
                <input
                  class="input"
                  placeholder="Thread-ID aus T3"
                  aria-label="Thread-ID"
                  value={adoptId()}
                  onInput={(event) => setAdoptId(event.currentTarget.value)}
                />
                <Button
                  size="sm"
                  disabled={!adoptId().trim()}
                  onClick={() => commands.resolveTask(task().id, { resolution: "adopt_thread", threadId: adoptId().trim() })}
                >
                  Zuordnen
                </Button>
                <Button size="sm" onClick={() => commands.resolveTask(task().id, { resolution: "not_sent" })}>
                  Neu senden
                </Button>
              </div>
            </Match>
            <Match when={task().status === "failed"}>
              <p class="task-callout">{task().error ?? "Der Agent hat abgebrochen."}</p>
            </Match>
            <Match when={task().lastMessage}>
              <p class="task-message">{task().lastMessage}</p>
            </Match>
          </Switch>

          <Show when={task().workspaceConflict}>
            <p class="task-callout">
              <Icon name="warn" size={14} /> Ein weiterer Agent schreibt in denselben Ordner.
            </p>
          </Show>

          <p class="task-facts">
            <span>{task().profile.name}</span>
            <span>{task().branch ?? strategyLabel[task().profile.workspaceStrategy]}</span>
            <span>{timeAgo(task().dispatchedAt ?? task().createdAt)}</span>
          </p>

          <Show when={task().threadId && task().status !== "unknown_remote_state"}>
            <form
              class="task-follow"
              onSubmit={(event) => {
                event.preventDefault();
                void sendFollowUp();
              }}
            >
              <input
                class="input"
                placeholder="Nachtrag an diesen Agenten"
                aria-label="Nachtrag"
                value={followUp()}
                onInput={(event) => setFollowUp(event.currentTarget.value)}
              />
              <Button size="sm" type="submit" disabled={!followUp().trim()} aria-label="Nachtrag senden">
                <Icon name="send" size={14} />
              </Button>
            </form>
          </Show>

          <div class="task-actions">
            <Show when={task().status === "failed"}>
              <Button size="sm" onClick={() => commands.retryTask(task().id)}>
                <Icon name="retry" size={14} /> Erneut senden
              </Button>
            </Show>
            <Show when={task().status === "completed"}>
              <Button size="sm" onClick={() => commands.patchTask(task().id, { markedDone: !task().markedDone })}>
                <Icon name="check" size={14} /> {task().markedDone ? "Wieder öffnen" : "Erledigt"}
              </Button>
            </Show>
            <Show when={!settled() && task().status !== "unknown_remote_state" && task().status !== "failed"}>
              <Button size="sm" variant="ghost" onClick={() => commands.cancelTask(task().id)}>
                Abbrechen
              </Button>
            </Show>
            <Show when={task().threadUrl}>
              {(url) => (
                <a class="btn btn-ghost btn-sm" href={url()} target="_blank" rel="noreferrer">
                  In T3 öffnen <Icon name="external" size={14} />
                </a>
              )}
            </Show>
          </div>
        </div>
      </Show>
    </li>
  );
}
