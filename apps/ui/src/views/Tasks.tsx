import type { Task } from "@zuruf/protocol";
import { For, Show, createMemo } from "solid-js";
import { TaskRow } from "../components/TaskRow";
import type { AppStore } from "../lib/store";

const NEEDS_YOU = new Set<Task["status"]>(["needs_approval", "needs_review", "failed", "unknown_remote_state"]);
const DONE = new Set<Task["status"]>(["completed", "cancelled"]);

/** The board, ordered by what it asks of the user: first what waits on them, then what runs, then what is done. */
export function Tasks(props: { store: AppStore }) {
  const groups = createMemo(() => {
    const all = props.store.tasks();
    return [
      { title: "Braucht dich", tasks: all.filter((task) => NEEDS_YOU.has(task.status)) },
      { title: "Läuft", tasks: all.filter((task) => !NEEDS_YOU.has(task.status) && !DONE.has(task.status)) },
      { title: "Fertig", tasks: all.filter((task) => DONE.has(task.status)) },
    ].filter((group) => group.tasks.length > 0);
  });

  return (
    <div class="page">
      <header class="page-head">
        <h1>Aufgaben</h1>
      </header>
      <Show
        when={groups().length > 0}
        fallback={<p class="page-empty">Noch nichts gesendet. Aufnahmen landen hier, sobald du sie abschickst.</p>}
      >
        <For each={groups()}>
          {(group) => (
            <section class="group">
              <h2 class="group-title">{group.title}</h2>
              <ul class="tasks tasks-board">
                <For each={group.tasks}>{(task) => <TaskRow task={task} store={props.store} />}</For>
              </ul>
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}
