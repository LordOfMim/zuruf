import { For, Show, createMemo } from "solid-js";
import { TaskRow } from "../components/TaskRow";
import type { AppStore } from "../lib/store";
import { taskGroup } from "../lib/tasks";

/** The board, ordered by what it asks of the user: first what waits on them, then what runs, then what is done. */
export function Tasks(props: { store: AppStore }) {
  // Fixed groups: deriving them as fresh objects would remount every row on each update.
  const all = () => props.store.tasks();
  const groups = [
    { title: "Braucht dich", tasks: createMemo(() => all().filter((task) => taskGroup(task) === "attention")) },
    { title: "Läuft", tasks: createMemo(() => all().filter((task) => taskGroup(task) === "active")) },
    { title: "Fertig", tasks: createMemo(() => all().filter((task) => taskGroup(task) === "finished")) },
    { title: "Abgeschlossen", tasks: createMemo(() => all().filter((task) => taskGroup(task) === "settled")) },
  ];

  return (
    <div class="page">
      <header class="page-head">
        <h1>Aufgaben</h1>
      </header>
      <Show
        when={all().length > 0}
        fallback={<p class="page-empty">Noch nichts gesendet. Aufnahmen landen hier, sobald du sie abschickst.</p>}
      >
        <For each={groups}>
          {(group) => (
            <Show when={group.tasks().length > 0}>
              <section class="group">
                <h2 class="group-title">{group.title}</h2>
                <ul class="tasks tasks-board">
                  <For each={group.tasks()}>{(task) => <TaskRow task={task} store={props.store} />}</For>
                </ul>
              </section>
            </Show>
          )}
        </For>
      </Show>
    </div>
  );
}
