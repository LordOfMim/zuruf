# Zuruf – Benutzerhandbuch

## Installieren

Voraussetzungen: Linux mit Hyprland/Wayland, Chromium, PipeWire, Voxtype mit mehrsprachigem Modell für Deutsch und T3 Code. Für den Build zusätzlich Rust stable über mise, Node 26, pnpm 11, Python 3 und eine erreichbare systemd-Benutzersitzung.

Im Checkout `./install.sh` ausführen. Das Skript baut UI, Extension und Release-Binary unter einer Speichergrenze von 3 GiB (Cargo: zwei Jobs). Es installiert ausschließlich im Benutzerbereich, aktiviert jedoch keinen Dienst. Erneuter Aufruf ersetzt Programm und Bundles, vorhandene Daten bleiben erhalten. `--skip-build` verwendet bereits gebaute Dateien. Das Platzhaltericon unter `assets/icon/zuruf.svg` kann vor der Installation ersetzt werden.

Standardziele sind `~/.local/bin/zuruf`, `~/.local/share/zuruf/{ui,extension}`, `~/.config/systemd/user/zuruf.service`, der Desktopstarter und das hicolor-Icon. Die UI wird außerdem bereits beim Rust-Build eingebettet; die zusätzlichen installierten Bundles erfüllen den Installationsvertrag. Die endgültige Nutzung der Extensiondateien durch den echten Browserstart **folgt** mit der Verdrahtung.

`PREFIX` sowie `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME` ändern Installationsziele. Der Dienst erhält die aufgelösten `ZURUF_DATA_DIR`, `ZURUF_STATE_DIR` und `ZURUF_CONFIG_DIR` ausdrücklich in der Unit. Die CLI benötigt bei abweichenden Datenpfaden dieselben Variablen. `.env.example` dokumentiert Variablen, wird aber nicht automatisch eingelesen.

## Erster Start

Zuerst `zuruf doctor` ausführen. `ok`, `fehlt` und `Hinweis` erklären Abhängigkeiten; fehlende Pflichtbestandteile führen zu Exitcode 1. Der Check lädt keine Modelle herunter und verändert weder Voxtype noch Hyprland.

Dieser Branch enthält noch den Fake-Daemon: Ein echter Start ohne `ZURUF_FAKE_ADAPTERS=1` wird ausdrücklich abgewiesen. Produktiver Browser-/Audio-/T3-Betrieb **folgt** mit der parallelen Adapterverdrahtung. Den Dienst deshalb erst danach aktivieren:

```sh
systemctl --user daemon-reload
systemctl --user enable --now zuruf.service
zuruf open
```

`zuruf open` fordert aktuell einen kurzlebigen Link an und gibt ihn aus. Automatischer Dienststart und eigenes App-Fenster **folgen**. Die installierte Unit hat Speichergrenzen, startet bei Fehlern neu und ist an die grafische Sitzung gebunden. Keine globalen Konfigurationsdateien werden geändert.

## Mit T3 verbinden

Pairing/OAuth über die bestehende T3-Schnittstelle ist vorgesehen. Die vollständige Bedienung aus Zuruf **folgt**; keine T3-Dateien oder Datenbanken manuell bearbeiten. Die Agentenübergabe mit taskgebundenem MCP-Zugang ist in `docs/mcp.md` beschrieben. Automatische Credential-Injection in T3 ist in diesem Branch nicht verdrahtet.

## Review aufnehmen

Vorgesehener Ablauf nach Verdrahtung: Projekt und Session wählen, manuell im verwalteten Browser anmelden, Review aktivieren. `W` erfasst den Viewport, `S` einen Ausschnitt, `D` ein DOM-Element. Die Extension unterdrückt diese Kurzbefehle bei Texteingaben. `R` steuert die Aufnahme; gesprochener Text wird über Marker zugeordnet. `Shift+N` versiegelt den Batch und öffnet sofort einen neuen Puffer. Dispatch und Transkription laufen asynchron. Eine Audio-Lücke des Voxtype-Fallbacks muss sichtbar bleiben.

Domänenlogik, Fake-Flows sowie Browser-/Sprachadapter sind vorhanden; die gemeinsame Abnahme dieses Ablaufs im laufenden Desktop **folgt**. Vor Versand Screenshots und Freitext auf sensible Inhalte prüfen.

## Aufgabenboard und Sessions

Tasks haben eigene Batches und können parallel laufen. Das Modell kennt Status, letzte Nachricht, Fehler und Worktree-Konflikte. Das endgültig verdrahtete Board **folgt**. MCP-Agenten können ihren zugewiesenen Kontext lesen, Annotationstatus aktualisieren und Fortschritt melden.

Sessions besitzen getrennte IDs und Browserprofile. Der Mensch hat Vorrang bei Takeover. MCP gibt den gefilterten CDP-Endpunkt nur während einer eigenen Lease heraus. Die atomare Verlängerung einer Lease fehlt noch im Browservertrag und wird derzeit mit einem Toolfehler abgewiesen. Reale CDP-Autorisierung und sofortiger Befehlsabbruch benötigen die parallele Proxyverdrahtung.

## Kurzbefehle ändern

Bindings sind über die vorhandene Settings-API konfigurierbar und werden auf Konflikte geprüft. Die fertig integrierte Einstellungsbedienung **folgt**. Seitenaktionen erlauben Einzeltasten; globale Aktionen benötigen einen Modifier. Hyprland-Konfiguration nicht manuell für Zuruf umschreiben.

## Deinstallieren

`./uninstall.sh` stoppt/deaktiviert bei Standardinstallation den Benutzerdienst und entfernt Binary, Unit, Starter, Icon sowie installierte Bundles. Benutzerprojekte, Datenbank, Profile und Einstellungen bleiben erhalten. Anschließend bei Standardinstallation `systemctl --user daemon-reload` ausführen.

`./uninstall.sh --purge` fragt vor dem Löschen sämtlicher Zuruf-Daten, Zustände und Einstellungen nach der ausdrücklichen Eingabe `LÖSCHEN`. Bei abweichenden Installationszielen dieselben Variablen wie beim Installieren setzen. Isolierte Testziele steuern niemals die echte systemd-Sitzung.

## Fehlersuche

`zuruf doctor` prüft Chromium-/Voxtype-/T3-Versionen, T3-Port 3773, Hyprland- und PipeWire-Socket, Modelldateien im Voxtype-Standardverzeichnis und verfügbaren RAM. Abweichende Voxtype-Modellpfade manuell prüfen. Ein Socket-/Portcheck bestätigt noch keine erfolgreiche Anmeldung oder Aufnahme. Bei wenig freiem RAM Browser-Sessions schließen. `journalctl --user -u zuruf.service` zeigt Dienstfehler; niemals Tokens veröffentlichen.

`tests/install-roundtrip.sh` prüft eine isolierte Installation einschließlich Upgrade, Unit-/Desktopsyntax, Datenerhalt und bestätigtem Purge. Das setzt fertige Release-/Frontend-Builds voraus. Alle Testläufe speicherbegrenzt starten, wie in `docs/mcp.md` beschrieben.
