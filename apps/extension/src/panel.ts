import type { Bootstrap } from "./protocol";
async function load(): Promise<void> {
  const bootstrap: Bootstrap = await (
    await fetch(chrome.runtime.getURL("bootstrap.json"))
  ).json();
  // Keep credentials in the extension world. The UI bundle can fetch the same file.
  if (bootstrap.protocolVersion !== 1)
    throw new Error("Unbekannte Protokollversion");
  try {
    const response = await fetch(chrome.runtime.getURL("panel/entry.js"));
    if (response.ok) {
      const script = document.createElement("script");
      script.src = chrome.runtime.getURL("panel/entry.js");
      document.body.append(script);
    }
  } catch {
    /* UI build is optional during development. */
  }
}
void load();
