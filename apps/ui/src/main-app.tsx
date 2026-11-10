import { boot } from "./boot";
import { App } from "./views/App";

void boot((store) => <App store={store} />);
