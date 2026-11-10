# Browser-Spikes für Zuruf

Kleine Laborprogramme, keine Anwendung. Nur synthetische Fixture-Daten. Keine Browserdownloads und keine Änderungen an Benutzerprofilen oder Hyprland-Konfiguration. Der Hauptlauf öffnet nacheinander drei eigene Wayland-Fenster für insgesamt etwa zehn Sekunden und schließt sie wieder. Nicht gleichzeitig ausführen.

```bash
cd /home/mika/Projects/Humaify/zuruf/main
pnpm --dir /home/mika/Projects/Humaify/zuruf/main/spikes/browser install --frozen-lockfile
systemd-run --user --wait --collect --pipe --working-directory=/home/mika/Projects/Humaify/zuruf/main -p MemoryMax=3G -p MemorySwapMax=1G -- /usr/bin/node /home/mika/Projects/Humaify/zuruf/main/spikes/browser/proof.ts
systemd-run --user --wait --collect --pipe --working-directory=/home/mika/Projects/Humaify/zuruf/main -p MemoryMax=3G -p MemorySwapMax=1G -- /usr/bin/node /home/mika/Projects/Humaify/zuruf/main/spikes/browser/pipe.ts
systemd-run --user --wait --collect --pipe --working-directory=/home/mika/Projects/Humaify/zuruf/main -p MemoryMax=3G -p MemorySwapMax=1G -- /usr/bin/node /home/mika/Projects/Humaify/zuruf/main/spikes/browser/hyprland.ts
systemd-run --user --wait --collect --pipe --working-directory=/home/mika/Projects/Humaify/zuruf/main -p MemoryMax=3G -p MemorySwapMax=1G -- /home/mika/.local/bin/pnpm --dir /home/mika/Projects/Humaify/zuruf/main/spikes/browser check
```

`proof.ts` startet zusätzlich jeden Chromium in einer eigenen begrenzten systemd-Unit. HTTP und WebSocket binden an `127.0.0.1:0`; Chromium ebenfalls an einen zufälligen Loopback-Port. Der Server endet im `finally`, Browser über `Browser.close`, bei Timeout über die genaue eigene Unit. Normale Fehler räumen auf. Nach hartem Abbruch eigene übrig gebliebene Units über `systemctl --user list-units 'zuruf-browser-*'` identifizieren und gezielt stoppen; niemals fremde Chromium-Prozesse beenden.

`proof.ts`: Extension laden, Hotkeys/Fokus, DOM/Locatoren/Redaktion, Shared CDP und Takeover, echte MCP-stdio-Verbindung, Side Panel, PNGs mit Ring, Drag-Auswahl mit DPR 1/2 und Browserzoom 100/125 %, Scrollposition, Cookie-Persistenz, Kontext-/Profilisolation, eigenes Fenster verschieben/vergrößern, separates `--app`-Fenster. DPR wird mit CDP emuliert, Browserzoom über `chrome.tabs.setZoom` gesetzt. `HEADLESS=1` muss als Environment an die systemd-Unit weitergereicht werden; Desktop-/Panel-Prüfungen sind dann nicht gleichwertig.

`pipe.ts`: Playwright startet System-Chromium über Pipe; `Extensions.loadUnpacked` lädt das generierte Extension-Verzeichnis aus dem vorausgegangenen Hauptlauf. Der Hauptserver muss dafür nicht mehr laufen.

`hyprland.ts`: prüft, dass die Testkombination frei ist, registriert temporär `SUPER+SHIFT+CTRL+ALT+F24` mit `/usr/bin/true`, prüft die Registrierung und entfernt sie im `finally`. Es drückt die Kombination nicht. Keine Konfigurationsdatei wird geschrieben.

`mcp-client.ts`: minimaler MCP-stdio-Client, wird vom Hauptlauf aufgerufen. Kein eigener Browser; verwendet den authentifizierten Proxy.

`extension/`: ungestylte Funktionsprobe; Review-Modus im Labor anfangs aktiv, W/S/D/P (Panel). `configure` kann Review abschalten und das Key-Mapping ersetzen. In einer Produktversion muss der Modus ausdrücklich aktiviert werden. Die Port-/Token-Platzhalter werden ausschließlich in die ignorierte Laufzeitkopie geschrieben. `fixture/` enthält Hauptseite und Frame; `/ui` ist absichtlich nur ein HTML-Platzhalter, keine React-Implementierung.

Ausgaben: `.spike/browser/results.json`, `pipe-results.json`, `hyprland-results.json`, PNGs und `run-<pid>/` mit ausschließlich eigenen Testprofilen. `evidence.json` enthält den bereinigten erfolgreichen Nachweislauf vom 10.11.2026; keine Zugangsdaten oder Cookie-Werte. Keine Profile/PNGs einchecken. Details und Grenzen: [browser.md](../../docs/feasibility/browser.md).
