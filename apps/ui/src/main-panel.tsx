import { boot } from "./boot";
import { Offline, Toasts } from "./views/App";
import { Panel } from "./views/Panel";

void boot((store) => (
  <div class="panel-root">
    <Offline store={store} />
    <Panel store={store} />
    <Toasts store={store} />
  </div>
));
