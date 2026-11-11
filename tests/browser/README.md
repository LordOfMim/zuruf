# Browser-Tests

`fixture/` enthält die Seiten, gegen die der Browser geprüft wird: Formularfelder, Editorflächen, Shadow DOM, ein eingebetteter Frame und Links, die Fenster öffnen.

| Prüfung | Aufruf | deckt ab |
|---|---|---|
| Sitzung im echten Fenster | `ZURUF_SHELL=target/debug/zuruf-shell cargo test -p zuruf-browser --test shell -- --ignored` | Start, Seiten-Agent, Hotkeys, Viewport-, Bereichs- und Elementaufnahme, Agentenproxy, Tabs, Takeover, Stopp |
| Tasten des Seiten-Agenten | `ZURUF_TEST_CDP=http://127.0.0.1:<port> pnpm --filter @zuruf/page test` | Einzeltasten, Textfelder, Alt-Varianten, eine Anfrage je Aufnahmetaste |
| Ganze App | `node tests/e2e/shell.mjs` | Daemon, eigenes Fenster, angedockte App, unverändertes Playwright als Agent, Absturz und Neustart, Fenster schließen |

Alle drei brauchen einen Bildschirm (X11 oder Wayland). Ohne Desktop laufen sie im Container aus `tools/dev/` unter Xvfb beziehungsweise sway; dort `ZURUF_SHELL_ARGS="--no-sandbox --disable-gpu"` setzen. Von Hand ausprobieren: `cargo run -p zuruf-browser --example session -- <Adresse>`.

Die Messwerte des früheren Aufbaus mit System-Chromium und Extension (Latenzen, Zoom-/DPR-Matrix) gelten für das eigene Fenster nicht mehr als belegt und sind neu zu erheben; siehe [docs/abnahme.md](../../docs/abnahme.md).
