import { Show } from "solid-js";
import { boot } from "./boot";
import { App, Offline, Toasts } from "./views/App";
import { Panel } from "./views/Panel";

// Two windows: the panel is only the review tool. One window: it carries the whole app.
void boot((store) => (
  <Show
    when={store.settings()?.ui.browser === "integrated"}
    fallback={
      <div class="panel-root">
        <Offline store={store} />
        <Panel store={store} />
        <Toasts store={store} />
      </div>
    }
  >
    <App store={store} docked />
  </Show>
));
