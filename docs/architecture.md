# Zuruf – Architektur

Verbindlich für alle Agenten. Grundlage: `docs/spec.md` und die verifizierten Befunde in `docs/feasibility/`. Wo dieses Dokument von der Spezifikation abweicht (Hyprland statt KDE, Rust statt Node, Solid statt React, eigener Browser statt Chromium mit Extension), gilt dieses Dokument.

## Leitlinien

- **Performance und wenig Speicher.** Zwei Rust-Programme, kein Node zur Laufzeit, kein Electron: `zuruf` (Daemon + CLI als Unterbefehle) und `zuruf-shell`, Zurufs eigenes Browserfenster mit eingebettetem Chromium (CEF). Die CLI bleibt klein und startet sofort, weil nur die Shell gegen Chromium gelinkt ist.
- **Ein Fenster, keine Reibung.** Die App sitzt im Browserfenster neben der geprüften Seite. Es gibt keine Extension, die geladen, verbunden oder aktuell gehalten werden muss: Alles, was in der Seite läuft, bringt die App selbst ein.
- **Nichts blockiert die Aufnahme.** Jede Benutzeraktion wird sofort lokal bestätigt und persistiert; Transkription, Dispatch und T3-Abfragen laufen in Hintergrund-Tasks.
- **Der Daemon ist die einzige Wahrheit.** UI, Seitenleiste, Browserfenster und CLI sind dünne Clients über genau einen Vertrag: `packages/protocol/src/index.ts`. Rust spiegelt ihn mit serde (camelCase). Änderungen am Vertrag nur über den Projektleiter.
- **Ehrliche Zustände.** Unklare Remote-Zustände, Audiolücken und nicht erreichbare Seiten werden angezeigt, nie verschwiegen oder geraten.

## Prozesse

```text
zuruf daemon  (systemd --user, 127.0.0.1:47600)
 ├─ HTTP/WS-API  /api/v1 …            ← App, Seitenleiste, CLI
 ├─ WS /cdp/<sessionId>               ← Agenten (Playwright MCP), gefiltert und an Kontrolle gebunden
 ├─ /mcp                              ← eigener MCP-Server für Agenten
 ├─ zuruf-shell je Browser-Session    (eigenes Profil, eigene systemd-Scope; stdin: Befehle, CDP: Seiten)
 ├─ zuruf-shell --zuruf-app-only      App-Fenster, solange keine Session die App trägt
 ├─ Sprach-Pipeline                   (Aufnahme + voxtype)
 └─ T3-Client                         (MCP über HTTP, CLI als Fallback)
zuruf <befehl>   kurzlebige CLI → spricht mit dem Daemon (Hyprland-Hotkeys rufen `zuruf action <id>`)
```

Der Daemon besitzt alle Fenster. `zuruf open` fragt nur an (`POST /app/open`): Läuft eine Session und ist „Ein Fenster“ eingestellt (Standard), holt der Daemon deren Fenster nach vorn; sonst zeigt er die App in einem eigenen Shell-Fenster. Startet eine Session im Ein-Fenster-Modus, schließt der Daemon das eigene App-Fenster. Endet der Daemon, endet die Befehlsleitung der Shells (stdin) und sie schließen sich selbst – es bleiben keine verwaisten Browser zurück.

## Repository

```text
crates/
  zuruf-core      Domänentypen (= Protokoll), Zustandsautomaten für Batch/Task/Kontrolle, Manifest, Redaktion. Rein, ohne IO.
  zuruf-store     SQLite (rusqlite, Migrationen, WAL) + Asset-Dateiablage, atomare Schreibvorgänge.
  zuruf-browser   Shell-Start, CDP-Client, Seiten-Brücke, Capture-Pipeline, Kontroll-Arbitration, CDP-Proxy.
  zuruf-shell     Das Browserfenster: eingebettetes Chromium (CEF), App-Panel, Tabs, Adressleiste. Eigenes Programm.
  zuruf-speech    SpeechAdapter + Voxtype-Backends.
  zuruf-t3        T3Adapter: MCP/OAuth primär, CLI-Fallback, Capability Detection.
  zuruf-hypr      Hyprland: globale Binds (Lua/Legacy erkennen), Fenster finden/anordnen.
  zuruf-mcp       Eigener MCP-Server (Tools aus Spec §9).
  zuruf           Binary: Daemon (axum), Dispatch-Queue, Verdrahtung, CLI-Unterbefehle, eingebettete UI.
apps/
  page            Seiten-Agent (TypeScript): Overlay, Hotkeys, Element- und Bereichsauswahl. Ein Skript, in `zuruf-browser` eingebettet.
  ui              Oberfläche (SolidJS + Vite): App-Fenster und Seitenleiste aus einer Codebasis, vom Daemon ausgeliefert.
packages/
  protocol        Der Vertrag (TypeScript-Typen + Routenliste).
```

Abhängigkeiten schlank halten: tokio, axum, serde, rusqlite (bundled), tokio-tungstenite, reqwest (rustls), uuid (v7), time, png, thiserror/anyhow, tracing, clap, rust-embed. Kein chromiumoxide/headless_chrome – der CDP-Client ist ein kleiner eigener JSON-RPC-Client über WebSocket. Nur `zuruf-shell` hängt von `cef` ab; dessen Build lädt die passende Chromium-Laufzeit (CEF) und braucht `cmake` und `ninja`.

## Datenablage

```text
~/.local/share/zuruf/            (0700)
  zuruf.sqlite
  projects/<projectId>/reviews/<reviewId>/batches/<batchId>/
    batch.json                   Manifest (Spec §8.3, schemaVersion "1.0"), beim Versiegeln geschrieben
    assets/ann-<n>.png, ann-<n>-original.png, ann-<n>-crop.png
    transcripts/segment-<n>.txt
  sessions/<sessionId>/profile/  Browserprofil der Session
  sessions/<sessionId>/shell.log Chromium-Protokoll des letzten Starts
  app-window/profile/            Profil des eigenen App-Fensters
~/.local/state/zuruf/            (0700) token, t3-credentials.json, Logs
~/.config/zuruf/settings.json    lesbare Einstellungen inkl. Tastenbelegung
```

Entitäten wie in Spec §12 mit UUIDv7, Zeitstempeln, Statushistorie (`task_events`) und Schema-Migrationen. Jede Zustandsänderung wird vor der Bestätigung an den Client committet; nach einem Absturz stellt der Daemon offenen Batch, unversendete Captures, Outbox und Thread-Zuordnungen wieder her (AT-18).

## Authentifizierung (lokal)

- Daemon bindet nur `127.0.0.1`. Ein zufälliges Token liegt in `~/.local/state/zuruf/token` (0600).
- CLI liest die Datei. App-Fenster und Seitenleiste lädt der Daemon selbst in die Shell und gibt dabei einen Einmalcode im URL-Fragment mit, den die Seite gegen das Token tauscht (`POST /auth/exchange`). Es gibt kein zweites, sessiongebundenes Token mehr.
- Alle HTTP-Aufrufe tragen `Authorization: Bearer`; WebSockets senden als erste Nachricht `{type:"auth",token}`. Origin wird geprüft: nur der eigene Ursprung, keine CORS-Freigaben. Agenten-Token für `/cdp` und `/mcp` sind getrennte, widerrufbare Token mit Session-Bindung.

## Browser

- **Shell.** Je Session ein `zuruf-shell`-Prozess mit eigenem Profil (`--zuruf-profile`), gestartet in einer eigenen systemd-Scope mit Speichergrenze, wo es eine systemd-Benutzersitzung gibt. Ein Fenster: links das App-Panel (`--zuruf-panel`, vom Daemon ausgeliefert), rechts Tab-Leiste, Adressleiste und die sichtbare Seite. Unter Wayland wählt die Shell selbst die Wayland-Plattform; ihr Fenster trägt die App-ID `zuruf`. Seiten, die ein Fenster öffnen, bekommen einen Tab und behalten ihren Opener.
- **Befehle an die Shell:** eine JSON-Zeile je Befehl auf stdin (`open`, `focus`, `panel`, `theme`, `quit`), Ereignisse als Zeilen auf stdout (`ready`, `closed`). Schließt der Mensch das Fenster, meldet die Shell `closed` und die Session gilt als gestoppt; verschwindet sie ohne Meldung, gilt sie als abgestürzt. In beiden Fällen lässt sie sich einfach neu starten.
- **Seiten erreicht der Daemon über CDP** (Loopback-Port, den der Daemon wählt und nie herausgibt). Das eigene App-Fenster hat keinen Debugging-Endpunkt.
- **Seiten-Agent statt Extension:** `apps/page` ist ein einzelnes Skript, das der Daemon über CDP in eine isolierte Welt jedes Frames einbringt (`Page.addScriptToEvaluateOnNewDocument`, Welt `zuruf`), auch in Frames anderer Prozesse. Es meldet Tastendrücke, Zeigerposition, Titel sowie Picker- und Rechteck-Ergebnisse über ein Binding, das nur in dieser Welt existiert; Befehle kommen per Auswertung in derselben Welt zurück. Webseiten können den Agenten weder sehen noch aufrufen. Jedes Dokument hat eine Kennung (der Ausführungskontext); eine Aufnahme ist an sie gebunden und wird verworfen, wenn die Seite inzwischen gewechselt hat.
- **Hotkeys in der Seite:** Capture-Phase auf `window`; aktiv nur im Review-Modus; nie bei Fokus in input/textarea/select/contenteditable/ARIA-Textbox/Editor-Containern oder unbekannten fokussierten Custom Elements (geschlossene Shadow Roots → konservativ unterdrücken). Belegung kommt vom Daemon (`Binding[]`, Scope `page`) und wird live aktualisiert; ein neu geladenes Dokument bekommt sie zusammen mit Aufnahmestatus und Kontrollbesitzer.
- **Fenster-Kurzbefehle** gehören der Shell: Strg+T/W/L, Strg+Tab, Strg+R/F5, Alt+←/→, Strg+Plus/Minus/0, F12 (Entwicklerwerkzeuge in eigenem Fenster).
- **Capture-Pipeline:** Aktion → an aktiven Batch binden → Overlay ausblenden (von jedem Frame bestätigt) → Metriken lesen → `Page.captureScreenshot` (Region: Clip = (Rechteck + visualViewport-Offset) × Browserzoom aus `Page.getLayoutMetrics`, `scale:1`) → Rohbild atomar schreiben → Ringkopie mit dem `png`-Crate → Annotation + Assets in einer Transaktion → Event. Ziel: Annotation erscheint < 150 ms nach Tastendruck in der UI (Platzhalter sofort, Bild folgt).
- **DOM-Capture:** Locator-Kandidaten in der Reihenfolge Test-ID, Rolle+Name, eindeutiger Text, CSS, XPath, jeweils mit `stable` und Begründung; Accessible Name über CDP `Accessibility.getPartialAXTree` validiert; Attribut-Allowlist; Frame-/Shadow-Pfad; nicht auflösbare Pfade als solche markiert.
- **Gemeinsame Steuerung:** `ControlState` je Session. Agenten erreichen den Browser nur über `/cdp/<sessionId>`; der Proxy lässt ausschließlich Seiten-Targets der Session durch (nie das App-Panel, kein `Browser.close`), serialisiert Eingaben je Tab und prüft vor jedem Befehl Besitzer und Lease. `Target.createTarget` eines Agenten öffnet einen Tab im Session-Fenster; der Proxy meldet die neue Seite, bevor er antwortet – so funktioniert unverändertes Playwright (`connectOverCDP`, `newPage`).
- **Takeover (Produktentscheidung):** Der Mensch gewinnt immer und sofort. Neue Agentenbefehle werden ab diesem Moment abgewiesen, die Lease ist widerrufen. Bereits laufende Befehle dürfen bis zu 500 ms auslaufen (`settling: true`), danach `Page.stopLoading`; kein `Runtime.terminateExecution` als Standard. Abgewiesene Befehle werden nie nachgeholt. Kontrolle geht erst wieder an einen Agenten, wenn der Mensch sie freigibt.
- **Seitenleiste:** das App-Panel der Shell. Im Ein-Fenster-Modus trägt es die ganze App, sonst nur das Review-Werkzeug. `panel.toggle` blendet es ein und aus; die Seite bekommt dann die volle Breite.
- **Isolated** = eigenes Profilverzeichnis. Auth-Klon nur explizit und nie automatisch.
- **Bewusst nicht enthalten:** Downloads, Berechtigungsabfragen (Kamera, Mikrofon, Standort) und Passwortverwaltung. Das Fenster ist ein Review-Browser, kein Alltagsbrowser.

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

- `page`: im Review-Browser, vom Seiten-Agenten ausgewertet; Einzeltasten erlaubt.
- `global`: desktopweit über Hyprland (`hyprctl eval 'hl.bind(…)'` bzw. Legacy-Syntax nach Erkennung), ruft `zuruf action <id>`; braucht einen Modifier. Binds werden beim Daemon-Start und nach Hyprland-Reload neu gesetzt und beim Beenden exakt wieder entfernt. Die Hyprland-Konfiguration des Nutzers wird nicht verändert.

Jede Änderung wird geprüft (`BindingIssue`: Duplikat, fehlender Modifier, vom System belegt). Standard: W/S/D/E/R, Shift+N, Esc im Scope `page`; `review.toggle`, `control.takeover`, `app.open` global.

## Installation

`./install.sh`: Release-Build, `zuruf` nach `~/.local/bin`, die Shell mit Chromiums Laufzeit (`libcef.so`, Ressourcen, deutsche und englische Sprachdateien) nach `~/.local/lib/zuruf/`, `systemd --user`-Unit `zuruf.service`, Starter `~/.local/share/applications/zuruf.desktop` mit Icon. Die UI ist ins Binary eingebettet, der Seiten-Agent in die Browser-Bibliothek. `zuruf` findet die Shell neben sich oder unter `../lib/zuruf` (`ZURUF_SHELL` überschreibt das); die Shell findet ihre Bibliothek neben sich. `zuruf open` startet bei Bedarf den Daemon und zeigt die App. `./uninstall.sh` entfernt alles außer den Daten.

Die Chromium-Laufzeit lädt der Build einmal je Version nach `~/.cache/zuruf/cef` (`CEF_PATH`). Hinter einem TLS-prüfenden Proxy erledigt das `tools/dev/fetch-cef.mjs`. `tools/dev/` enthält außerdem einen Linux-Build-Container, in dem sich alles ohne Desktop bauen und unter einem virtuellen Bildschirm (X11 oder Wayland) testen lässt.

Zielplattform ist Linux. Die Shell selbst ist plattformneutral geschrieben; Daemon und Adapter (`zuruf-store`, `zuruf-t3`, `zuruf-speech`, `zuruf-hypr`) verwenden Unix-Schnittstellen. Ein Windows-Port steht aus.

## Zuständigkeiten

| Bereich | Wer |
|---|---|
| Vertrag, Architektur, UX, `apps/ui`, Overlay-Gestaltung, Abnahme | Projektleiter (Claude) |
| `zuruf-core`, `zuruf-store`, `zuruf` (Daemon, Dispatch, CLI) | Codex-Agent „Kern“ |
| `zuruf-browser`, `zuruf-shell`, `apps/page` (Funktion) | Codex-Agent „Browser“ |
| `zuruf-t3`, `zuruf-speech`, `zuruf-hypr` | Codex-Agent „Adapter“ |
| Verdrahtung, `zuruf-mcp`, Installation, End-to-End-Tests | zweite Welle |
