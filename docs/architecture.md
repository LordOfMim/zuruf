# Zuruf – Architektur

Verbindlich für alle Agenten. Grundlage: `docs/spec.md` und die verifizierten Befunde in `docs/feasibility/`. Wo dieses Dokument von der Spezifikation abweicht (Hyprland statt KDE, Rust statt Node, Solid statt React), gilt dieses Dokument.

## Leitlinien

- **Performance und wenig Speicher.** Ein Rust-Binary `zuruf` (Daemon + CLI als Unterbefehle), kein Node zur Laufzeit, kein Electron. System-Chromium ist die einzige Browser-Engine.
- **Nichts blockiert die Aufnahme.** Jede Benutzeraktion wird sofort lokal bestätigt und persistiert; Transkription, Dispatch und T3-Abfragen laufen in Hintergrund-Tasks.
- **Der Daemon ist die einzige Wahrheit.** UI, Seitenleiste, Extension und CLI sind dünne Clients über genau einen Vertrag: `packages/protocol/src/index.ts`. Rust spiegelt ihn mit serde (camelCase). Änderungen am Vertrag nur über den Projektleiter.
- **Ehrliche Zustände.** Unklare Remote-Zustände, Audiolücken und nicht erreichbare Seiten werden angezeigt, nie verschwiegen oder geraten.

## Prozesse

```text
zuruf daemon  (systemd --user, 127.0.0.1:47600)
 ├─ HTTP/WS-API  /api/v1 …            ← UI-Fenster, Seitenleiste, CLI
 ├─ WS /ext                           ← Extension-Service-Worker je Browser-Session
 ├─ WS /cdp/<sessionId>               ← Agenten (Playwright MCP), gefiltert und an Kontrolle gebunden
 ├─ /mcp                              ← eigener MCP-Server für Agenten
 ├─ Chromium je Browser-Session       (eigenes Profil, eigene systemd-Scope, Extension geladen)
 ├─ Chromium --app für das UI-Fenster (eigenes kleines Profil, nie für Agenten erreichbar)
 ├─ Sprach-Pipeline                   (Aufnahme + voxtype)
 └─ T3-Client                         (MCP über HTTP, CLI als Fallback)
zuruf <befehl>   kurzlebige CLI → spricht mit dem Daemon (Hyprland-Hotkeys rufen `zuruf action <id>`)
```

## Repository

```text
crates/
  zuruf-core      Domänentypen (= Protokoll), Zustandsautomaten für Batch/Task/Kontrolle, Manifest, Redaktion. Rein, ohne IO.
  zuruf-store     SQLite (rusqlite, Migrationen, WAL) + Asset-Dateiablage, atomare Schreibvorgänge.
  zuruf-browser   Chromium-Start, CDP-Client, Capture-Pipeline, Kontroll-Arbitration, CDP-Proxy, Extension-Protokoll.
  zuruf-speech    SpeechAdapter + Voxtype-Backends.
  zuruf-t3        T3Adapter: MCP/OAuth primär, CLI-Fallback, Capability Detection.
  zuruf-hypr      Hyprland: globale Binds (Lua/Legacy erkennen), Fenster finden/anordnen.
  zuruf-mcp       Eigener MCP-Server (Tools aus Spec §9).
  zuruf           Binary: Daemon (axum), Dispatch-Queue, Verdrahtung, CLI-Unterbefehle, eingebettete UI/Extension.
apps/
  extension       MV3-Extension (TypeScript): Content-Script-Overlay, Service Worker, Panel-Hülle.
  ui              Oberfläche (SolidJS + Vite): App-Fenster und Seitenleiste aus einer Codebasis.
packages/
  protocol        Der Vertrag (TypeScript-Typen + Routenliste).
```

Abhängigkeiten schlank halten: tokio, axum, serde, rusqlite (bundled), tokio-tungstenite, reqwest (rustls), uuid (v7), time, png, thiserror/anyhow, tracing, clap, rust-embed. Kein chromiumoxide/headless_chrome – der CDP-Client ist ein kleiner eigener JSON-RPC-Client über WebSocket.

## Datenablage

```text
~/.local/share/zuruf/            (0700)
  zuruf.sqlite
  projects/<projectId>/reviews/<reviewId>/batches/<batchId>/
    batch.json                   Manifest (Spec §8.3, schemaVersion "1.0"), beim Versiegeln geschrieben
    assets/ann-<n>.png, ann-<n>-original.png, ann-<n>-crop.png
    transcripts/segment-<n>.txt
  sessions/<sessionId>/profile/  Chromium-Profil
  sessions/<sessionId>/extension/ Laufzeitkopie der Extension inkl. bootstrap.json
  ui-profile/                    Profil des App-Fensters
~/.local/state/zuruf/            (0700) token, t3-credentials.json, Logs
~/.config/zuruf/settings.json    lesbare Einstellungen inkl. Tastenbelegung
```

Entitäten wie in Spec §12 mit UUIDv7, Zeitstempeln, Statushistorie (`task_events`) und Schema-Migrationen. Jede Zustandsänderung wird vor der Bestätigung an den Client committet; nach einem Absturz stellt der Daemon offenen Batch, unversendete Captures, Outbox und Thread-Zuordnungen wieder her (AT-18).

## Authentifizierung (lokal)

- Daemon bindet nur `127.0.0.1`. Ein zufälliges Token liegt in `~/.local/state/zuruf/token` (0600).
- CLI liest die Datei. Das UI-Fenster erhält über `zuruf open` einen Einmalcode im URL-Fragment und tauscht ihn gegen das Token (`POST /auth/exchange`). Die Extension bekommt Port und ein sessiongebundenes Token über `bootstrap.json` in ihrer Laufzeitkopie; die Seitenleiste (Extension-Seite) liest es von dort.
- Alle HTTP-Aufrufe tragen `Authorization: Bearer`; WebSockets senden als erste Nachricht `{type:"auth",token}`. Origin wird geprüft (`chrome-extension://<id>` bzw. eigener Ursprung). Agenten-Token für `/cdp` und `/mcp` sind getrennte, widerrufbare Token mit Session-Bindung.

## Browser

- Je Session ein Chromium-Prozess: `/usr/bin/chromium --ozone-platform=wayland --user-data-dir=… --load-extension=… --remote-debugging-port=0` (Loopback), gestartet in einer eigenen systemd-Scope mit Speichergrenze. Der rohe CDP-Port wird nie herausgegeben. Ausbaustufe: `--remote-debugging-pipe`, sobald der Proxy alle Clients multiplexen kann.
- **Extension ↔ Daemon:** Service Worker hält einen authentifizierten WebSocket zu `/ext` (Heartbeat, Resume nach Worker-Schlaf, Quittungen, Dedup per `requestId`). Content Scripts (isolated world, `document_start`, alle Frames) liefern Tastendrücke, Zeigerposition, Picker- und Rechteck-Ergebnisse. Webseiten können keine Daemon-Befehle auslösen.
- **Hotkeys in der Seite:** Capture-Phase auf `window`; aktiv nur im Review-Modus; nie bei Fokus in input/textarea/select/contenteditable/ARIA-Textbox/Editor-Containern oder unbekannten fokussierten Custom Elements (geschlossene Shadow Roots → konservativ unterdrücken). Belegung kommt vom Daemon (`Binding[]`, Scope `page`) und wird live aktualisiert.
- **Capture-Pipeline:** Aktion → an aktiven Batch binden → Overlay ausblenden (quittiert) → Metriken lesen → `Page.captureScreenshot` (Region: Clip = (Rechteck + visualViewport-Offset) × Browserzoom, `scale:1`) → Rohbild atomar schreiben → Ringkopie mit dem `png`-Crate → Annotation + Assets in einer Transaktion → Event. Ziel: Annotation erscheint < 150 ms nach Tastendruck in der UI (Platzhalter sofort, Bild folgt).
- **DOM-Capture:** Locator-Kandidaten in der Reihenfolge Test-ID, Rolle+Name, eindeutiger Text, CSS, XPath, jeweils mit `stable` und Begründung; Accessible Name über CDP `Accessibility.getPartialAXTree` validiert; Attribut-Allowlist; Frame-/Shadow-Pfad; nicht auflösbare Pfade als solche markiert.
- **Gemeinsame Steuerung:** `ControlState` je Session. Agenten erreichen den Browser nur über `/cdp/<sessionId>`; der Proxy lässt ausschließlich Seiten-Targets der Session durch (keine Extension-/Panel-Targets, kein `Browser.close`), serialisiert Eingaben je Tab und prüft vor jedem Befehl Besitzer und Lease.
- **Takeover (Produktentscheidung):** Der Mensch gewinnt immer und sofort. Neue Agentenbefehle werden ab diesem Moment abgewiesen, die Lease ist widerrufen. Bereits laufende Befehle dürfen bis zu 500 ms auslaufen (`settling: true`), danach `Page.stopLoading`; kein `Runtime.terminateExecution` als Standard. Abgewiesene Befehle werden nie nachgeholt. Kontrolle geht erst wieder an einen Agenten, wenn der Mensch sie freigibt.
- **Seitenleiste:** Extension-Seite als `sidePanel`; öffnet sich durch die Toolbar-Schaltfläche, den Shortcut `panel.toggle` oder den ersten Tastendruck im Review-Modus (Benutzergeste erforderlich). Sie lädt das Panel-Bundle der UI, das mit dem Daemon spricht.
- **Isolated** = eigenes Profilverzeichnis. Auth-Klon nur explizit und nie automatisch.

## Sprache

Zwei Backends hinter einem `SpeechAdapter`; welches aktiv ist, steht in den Einstellungen und im Status.

1. **continuous (bevorzugt, sobald verifiziert):** Der Daemon nimmt selbst lückenlos auf (PipeWire, 16 kHz mono), setzt bei jedem Capture und bei `Shift+N` exakte Marker, schneidet Segmente und transkribiert sie nebenläufig über `voxtype transcribe <wav>`. Keine Audiolücke, kein Warten. WAV-Schnipsel werden nach der Transkription gelöscht, außer `keepAudio` ist an.
2. **daemon (Fallback, verifiziert):** `voxtype record start --file=… --no-auto-submit --no-smart-auto-submit --no-osd` / `record stop --wait --json`. Stop → Finalize → Restart an jeder Grenze; die Lücke wird gemessen und als `lastGapMs` gemeldet.

Regeln: Sprache nach einem Capture gehört zu diesem Capture, bis zum nächsten Marker (`attach: "after"`, umstellbar). Sprache vor dem ersten Capture eines Batches wird Batch-Notiz. Transkripte, die nur aus Satzzeichen bestehen, gelten als leer. Der Originaltext eines Segments wird nie überschrieben; bearbeitet wird `Annotation.transcript`. Läuft bereits eine fremde Voxtype-Aufnahme, meldet der Adapter `busy_external` und greift nicht ein. Modell und Sprache werden pro Aufruf gesetzt (`--model`, `--language`); die Voxtype-Konfiguration bleibt unberührt.

## Batch, Task, Dispatch

- Genau ein Batch je Review ist `draft`. `batch.dispatch` (Shift+N) läuft in **einer** SQLite-Transaktion: Batch → `sealed`, Profil einfrieren, Task anlegen (`finalizing_transcript` oder `queued`), neuen leeren Draft anlegen und aktivieren, Sprach-Epoch wechseln. Die Antwort kommt sofort; T3 wird dabei nicht berührt. Ein leerer Batch wird nicht versendet.
- Ein Hintergrund-Worker je Task: Segmente abwarten → Manifest + Prompt schreiben → `queued` → `dispatching` → Thread anlegen → `running` → Status verfolgen. Keine globale Sperre; mehrere Tasks laufen parallel.
- **Idempotenz:** `clientRequestId` = Task-Id, vor dem ersten Versand persistiert und zusätzlich als Marker im Thread-Titel. Nach Timeout oder Abbruch erst die Threads des Projekts nach dem Marker durchsuchen; nur bei eindeutigem Ergebnis automatisch fortfahren, sonst `unknown_remote_state` mit manueller Auflösung (`adopt_thread` / `not_sent`).
- **Arbeitsverzeichnisse:** Standard `new_worktree`. Starten zwei schreibende Tasks im selben Checkout, wird `workspaceConflict` gesetzt und gewarnt. Kein automatisches Mergen.
- **Statusabbildung** wie in `docs/feasibility/t3-voxtype.md` §5. Antworten zählen nur, wenn sie zum verfolgten Run gehören.
- **Prompt:** nennt den absoluten Pfad zu `batch.json`, fordert Rückmeldung je Annotation und verbietet nicht dokumentierte Zusatzanforderungen (Spec §8.3). Cookies, Token und Profile erscheinen nie in Manifest oder Prompt.

## T3-Anbindung

Primär der dokumentierte Orchestrator-MCP über HTTP (`http://127.0.0.1:3773/mcp`, Origin aus T3 ermitteln, nicht fest verdrahten) mit OAuth/PKCE; der Pairing-Code wird lokal per `t3 auth pairing create` erzeugt. Zugangsdaten liegen nur in `~/.local/state/zuruf/` (0600) und werden bei „Trennen“ widerrufen. Fallback ist die CLI (`t3 thread start|send|show|list --json`) mit klar gemeldeten Einschränkungen (`T3Connection.capabilities`). Kein Zugriff auf T3-Dateien oder -Datenbanken. Statusverfolgung über eine dauerhafte Verbindung, nicht über CLI-Prozesse je Abfrage.

## Kurzbefehle

Eine Aktionsliste (`ActionId`), zwei Geltungsbereiche:

- `page`: im Review-Browser, vom Content Script ausgewertet; Einzeltasten erlaubt.
- `global`: desktopweit über Hyprland (`hyprctl eval 'hl.bind(…)'` bzw. Legacy-Syntax nach Erkennung), ruft `zuruf action <id>`; braucht einen Modifier. Binds werden beim Daemon-Start und nach Hyprland-Reload neu gesetzt und beim Beenden exakt wieder entfernt. Die Hyprland-Konfiguration des Nutzers wird nicht verändert.

Jede Änderung wird geprüft (`BindingIssue`: Duplikat, fehlender Modifier, vom System belegt). Standard: W/S/D/E/R, Shift+N, Esc im Scope `page`; `review.toggle`, `control.takeover`, `app.open` global.

## Installation

`./install.sh`: Release-Build, Binary nach `~/.local/bin/zuruf`, `systemd --user`-Unit `zuruf.service`, Starter `~/.local/share/applications/zuruf.desktop` mit Icon. UI- und Extension-Bundles sind ins Binary eingebettet. `zuruf open` startet bei Bedarf den Daemon und öffnet das App-Fenster. `./uninstall.sh` entfernt alles außer den Daten.

## Zuständigkeiten

| Bereich | Wer |
|---|---|
| Vertrag, Architektur, UX, `apps/ui`, Overlay-Gestaltung, Abnahme | Projektleiter (Claude) |
| `zuruf-core`, `zuruf-store`, `zuruf` (Daemon, Dispatch, CLI) | Codex-Agent „Kern“ |
| `zuruf-browser`, `apps/extension` (Funktion) | Codex-Agent „Browser“ |
| `zuruf-t3`, `zuruf-speech`, `zuruf-hypr` | Codex-Agent „Adapter“ |
| Verdrahtung, `zuruf-mcp`, Installation, End-to-End-Tests | zweite Welle |
