import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./styles/tokens.css";
import "./styles/app.css";
import { type JSX, Show, createEffect } from "solid-js";
import { render } from "solid-js/web";
import { createHttpTransport, loadBootstrap } from "./lib/api";
import { createDemoTransport } from "./lib/demo";
import { type AppStore, createAppStore } from "./lib/store";

function NoAccess() {
  return (
    <div class="gate">
      <span class="mark" />
      <p>
        Öffne Zuruf über den Starter oder mit <code>zuruf open</code>.
      </p>
    </div>
  );
}

/** Shared start-up for both entry points: find the daemon, connect, apply the theme, render. */
export async function boot(view: (store: AppStore) => JSX.Element) {
  const root = document.getElementById("root")!;
  const params = new URLSearchParams(location.search);
  const demo = params.has("demo");
  const forcedTheme = params.get("theme");
  const bootstrap = demo ? null : await loadBootstrap().catch(() => null);

  if (!demo && !bootstrap) {
    render(() => <NoAccess />, root);
    return;
  }

  const store = createAppStore(demo ? createDemoTransport(params.get("demo") === "empty") : createHttpTransport(bootstrap!));

  render(() => {
    createEffect(() => {
      const theme = forcedTheme ?? store.settings()?.ui.theme ?? "system";
      if (theme === "system") delete document.documentElement.dataset.theme;
      else document.documentElement.dataset.theme = theme;
    });
    return <Show when={store.state.ready}>{view(store)}</Show>;
  }, root);
}
