# Phase 0: T3 Code und Voxtype

Stand: 10.11.2026, reale Tests auf diesem Rechner. Nur dieses Repository wurde bearbeitet, keine Commits. Browser-Spike und dessen Bericht sind nicht Teil dieser Prüfung. Es wurden keine T3-Quellen, Konfigurationen oder Datenbanken verändert beziehungsweise direkt geöffnet. Authentifizierung lief ausschließlich über T3-CLI und öffentliche HTTP-Schnittstellen; kurzlebige Prüfzugänge wurden erzeugt und anschließend widerrufen. Keine Tokens im Repository.

## Ergebnis und belastbare Grenzen

P0 ist grundsätzlich machbar: Ein externer Node-Prozess hat einen echten Zuruf-Thread mit explizitem Modell erzeugt, laufenden und abgeschlossenen Zustand gelesen, `OK` empfangen und ein Follow-up mit Antwort `OK` gesendet. Ein zweiter Thread läuft in einem von T3 erzeugten Worktree und kann eine absolute Datei außerhalb des Checkouts lesen. Externe MCP-Discovery, HTTP-Lesen und WebSocket-RPC funktionieren ohne Agenten-Session-Token.

Voxtype kann eine Aufnahme gezielt in eine Datei schreiben und mit `stop --wait --json` deren Abschluss melden. Zwei kurze Mikrofonversuche, einmal Stop und einmal Cancel, wurden durchgeführt. Stille ergab hier **`.\n` statt eines leeren Transkripts**. Sprachqualität, deutsche Diktate, genaue Zeitsegmentierung und wirklich lückenlose Aufnahme sind damit nicht abgenommen.

Wichtigste Überraschung: Die installierte MCP-Tool-Signatur enthält `t3_thread_launch.clientRequestId`, obwohl die verlinkte main-Dokumentation diese Funktion als ohne Retry-Key beschreibt. Lokale Schemas sind maßgeblich; **Replay-Semantik von Launch wurde nicht getestet**. Ein normaler CLI-Bearer funktioniert für HTTP/WS, aber nicht unmittelbar für `/mcp`.

## Umgebung und Quellen

```text
$ t3 --version
t3 v0.1.7
$ voxtype --version
voxtype 1.1.1
$ node --version
v26.12.0
$ pnpm --version
11.18.0
$ readlink -f /home/mika/.local/bin/t3
/home/mika/.t3/bin/t3
$ systemctl --user is-active voxtype
active
$ t3 status
This machine
  Service        not installed
  T3 Connect     on · your T3 account
  Local network  off

Change access: `t3 up`.
```

Desktop ist die vorgegebene Hyprland/Wayland-Umgebung; keine KDE-Annahme für diese Adapter. Die laufende Desktop-App lauscht unter `127.0.0.1:3773`. Ihr CLI-Launcher mountet die vorhandene AppImage und startet das gebündelte `apps/server/dist/bin.mjs`. Das erklärt einen Teil der Startkosten. Nicht daraus ableiten, dass `t3 status` mit „Service not installed“ keine laufende Desktop-Instanz bedeutet.

Geprüfte offizielle Referenzen: [Orchestrator-MCP](https://github.com/pingdotgg/t3code/blob/main/docs/orchestration-v2/orchestrator-mcp-server.md), [Installation](https://github.com/pingdotgg/t3code/blob/main/docs/user/install.md), [Thread-Sidebar](https://github.com/pingdotgg/t3code/blob/main/docs/user/thread-sidebar.md), [Voxtype-Handbuch](https://github.com/peteonrails/voxtype/blob/dev/docs/USER_MANUAL.md). Die lokalen CLI-Hilfen und vom laufenden Server gelieferten MCP-Schemas sind die Versionsbelege. Zusätzlich wurden die **installierten gebündelten JS-Dateien read-only** nach Transport-/Auth-Schemas untersucht, ohne Code zu importieren oder zu ändern. Das ist kein Stabilitätsversprechen für interne APIs.

Vollständige unveränderte Hilfen: [T3](../../spikes/t3/help.txt), [Voxtype](../../spikes/voxtype/help.txt). Kleine eigenständige Node-Prüfprogramme und Bedienhinweise liegen unter [spikes/t3](../../spikes/t3/README.md) und [spikes/voxtype](../../spikes/voxtype/README.md). Kein zusätzlicher Server, Browser oder ASR-Prozess wurde gestartet.

## 1. CLI-Oberfläche und Auswahlparameter

Alle vier Thread-Befehle unterstützen `--base-dir`, `--json` und globale Hilfe-/Loggingflags. Die vollständigen Globalflags stehen in `help.txt`.

| Befehl | Argumente | zusätzliche Flags |
|---|---|---|
| `t3 thread start` | `<project> <message>` | `--title`, `--provider`, `--model`, `--worktree`, `--branch`, `--wait` |
| `t3 thread send` | `<thread> <message>` | `--wait` |
| `t3 thread show` | `<thread>` | `--messages <integer>` (Default 10) |
| `t3 thread list` | keine | `--project` (Default aktuelles Verzeichnis) |
| `t3 project add` | `<path>` | `--base-dir`, `--title` |
| `t3 project remove` | `<project>` | `--base-dir`, `--force` (löscht auch Threads) |
| `t3 project rename` | `<project> <title>` | `--base-dir` |

**Kein `t3 project list`**, kein JSON-Flag bei Projektmutationen. Projektmutationen wurden nur über Hilfe untersucht, nicht ausgeführt. `thread start` löst Projekt-ID oder Pfad innerhalb eines registrierten Projekts auf, nicht den Titel. Bei verschachtelten Projekten gewinnt laut installiertem CLI-Code der tiefste Pfad. `message = -` liest stdin; Node soll Argumentarrays beziehungsweise stdin verwenden, keine Shell-Interpolation.

`--provider` ist eine konkrete Instanz-ID, kein beliebiger Providername. Trotz etwas missverständlicher Hilfetexte müssen `--provider` und `--model` **zusammen** gesetzt sein; der lokale Code weist ein einzelnes Flag zurück. Ohne beide gilt das Projektdefault. Es gibt keine CLI-Flags für Modelloptionen/Reasoning, Runtime oder Interaction Mode. Der Test ergab `full-access/default`; das ist kein für neue Versionen zugesicherter Default und keine sichere Benutzerpräferenz.

Ohne `--worktree` bindet Start an den Projektroot. `--worktree <baseRef> --branch <name>` erzeugt und bindet einen neuen Checkout; `--branch` allein ist unzulässig. **Existing-Worktree-Auswahl fehlt in der CLI**. Root und neuer Worktree wurden tatsächlich getestet; bestehender Worktree nur im MCP-Schema bestätigt.

Die CLI hat keinen `clientRequestId`/Idempotenzparameter. Ein Fallback muss vor dem ersten Versand eine UUID persistieren, in den Titel aufnehmen und projektbezogen `thread list --json` abgleichen. Bei verlorenem Resultat und fehlendem Treffer nicht sofort erneut erstellen: Der erste Request kann noch verarbeitet werden. Serialisierte lokale Outbox plus Wiederabgleich; verbleibende Ungewissheit als `unknown_remote_state` mit manueller Auflösung behandeln. Titelabgleich allein bietet keine Exactly-once-Garantie.

## 2. Externe Authentifizierung, Provider und Modelle

### Nachgewiesene Wege

1. **CLI:** funktioniert lokal ohne manuelles Login. Laut installiertem Code liest sie die Origin aus `~/.t3/userdata/server-runtime.json`, stellt intern eine kurzlebige MCP-Clientsession aus und widerruft sie am Befehlsende. Zuruf soll weder diese Credential-Dateien noch Datenbankinhalte auslesen.
2. **HTTP/WS:** `t3 auth session issue --scope orchestration:read --scope settings:write --ttl 5m --label zuruf-feasibility-ws --json` wird im Node-Prozess aufgefangen, nicht protokolliert. Bearer-Header erlaubt `GET /api/projects` und `GET /api/orchestration/threads/<encodedId>` mit `x-t3-orchestration-protocol: 2`. Beide lieferten HTTP 200. Die Snapshot-Antwort besitzt `{snapshotSequence, projection}`. Der WS-Katalog wurde ebenfalls real gelesen.
3. **MCP:** Ein solcher allgemeiner Bearer liefert bei `/mcp` HTTP 401:

```json
{"error":"invalid_mcp_credential","message":"A valid T3 Code MCP credential is required."}
```

Der funktionierende externe Weg ist **OAuth Authorization Code + PKCE S256**. `GET /.well-known/oauth-authorization-server` lieferte:

```json
{"issuer":"http://127.0.0.1:3773","authorization_endpoint":"http://127.0.0.1:3773/oauth/mcp/authorize","token_endpoint":"http://127.0.0.1:3773/oauth/mcp/token","registration_endpoint":"http://127.0.0.1:3773/oauth/mcp/register","response_types_supported":["code"],"grant_types_supported":["authorization_code"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"scopes_supported":["orchestration:read","orchestration:operate"],"authorization_response_iss_parameter_supported":true}
```

Der Spike registriert einen Client mit Loopback-Redirect, erzeugt über `t3 auth pairing create --scope orchestration:read --ttl 5m --label zuruf-feasibility-mcp --json` einen Einmalcode, übergibt diesen ausschließlich im Prozess an `/oauth/mcp/decision` (`decision._tag = pairing-code`, `access = read-only`) und tauscht den zurückgegebenen Code am Tokenendpoint. Es wird kein Callbackserver benötigt: Decision liefert Redirect-JSON. Der Einmalcode ist `credential` in der CLI-Antwort. OAuth-Token, Pairing-Code, PKCE-Verifier und Redirect mit Code werden nie geloggt. Der genaue Ablauf ist ausführbar in `mcp-discovery.mjs`.

**Überraschung:** Der 5-Minuten-Pairing-Code erzeugte eine MCP-Session mit `expires_in = 2592000` (30 Tage). Das Token lebt nicht nur fünf Minuten! Alle eigenen Sessions wurden über `t3 auth session revoke <sessionId>` widerrufen. Produktzugang bewusst verwalten und beim Disconnect widerrufen. Persistente Credentials ausschließlich außerhalb des Repos, restriktive Rechte beziehungsweise Secret Store. `--bootstrap-fd` ist Serverstart-/Desktop-Bootstrap, kein vom Companion zu entwendender Zugang.

MCP braucht `Accept: application/json, text/event-stream`, `mcp-protocol-version: 2025-06-18` und nach Initialize den zurückgegebenen `mcp-session-id` für Folgeaufrufe. Ohne Sessionheader kam HTTP 400; mit Header funktionieren `tools/list`, `orchestrator_capabilities` und `t3_thread_read` von außen. Initialize:

```json
{"protocolVersion":"2025-06-18","capabilities":{"logging":{},"tools":{"listChanged":true},"completions":{}},"serverInfo":{"name":"T3 Code","version":"0.1.7"}}
```

### Dynamische Auswahl

`orchestrator_capabilities` liefert auch extern ohne Parentthread Modelle, Optionen, Runtime-Modi, Verfügbarkeitsgrenzen und Featureflags. Bei read-only: `parentThreadId: null`, `runtimeMode: approval-required`. Beispiel aus der realen Antwort:

```json
{"providerInstanceId":"codex","driverKind":"codex","displayName":"Codex","models":[{"id":"gpt-6.1-sol","options":[{"id":"reasoningEffort","values":["low","medium","high","xhigh","max","ultra"],"default":"low"},{"id":"serviceTier","values":["default","priority"],"default":"priority"}]},{"id":"gpt-6-astra","options":[{"id":"reasoningEffort","values":["low","medium","high","xhigh","max","ultra"],"default":"medium"},{"id":"serviceTier","values":["default","priority"],"default":"priority"}]},{"id":"gpt-6.1-terra","label":"GPT-6.1 Terra","options":[{"id":"reasoningEffort","values":["low","medium","high","xhigh","max","ultra"],"default":"low"},{"id":"serviceTier","values":["default","priority"],"default":"priority"}]},{"id":"gpt-6-luna","options":[{"id":"reasoningEffort","values":["low","medium","high","xhigh","max"],"default":"medium"},{"id":"serviceTier","values":["default","priority"],"default":"priority"}]}],"omittedModels":4,"canRunChildTask":true,"canRunCrossProviderChildTask":true,"constraints":[]}
```

Aktiv waren `codex`, `claudeAgent`, `claude_openrouter`; weitere Instanzen sind teils ausdrücklich deaktiviert. `search` filtert die Antwort: `search: mini` lieferte hier leere Modellarrays und ist kein Beleg für einen leeren Gesamtkatalog. `omittedModels` beachten, Suchfeld verwenden. Der Katalog enthält keine verifizierte Preisliste; `gpt-6-luna` wurde als kleines Modell gewählt, keine tatsächlichen Abrechnungskosten gemessen.

Alternativ liefert WS `server.getConfig` den vollständigen Katalog: `providers[].instanceId`, `enabled`, `status`, `models[].slug`, `models[].capabilities.optionDescriptors` mit erlaubten Werten und Defaults. Im Test 565635 Bytes für die gesamte Konfiguration; nur Provider-Metadaten protokollieren. Nicht ungefiltert Settings/Secrets weiterreichen. `server.refreshProviders` existiert mit `instanceId`, `cwd`, `fresh`, `refreshModels`, `reloadSecrets`, `workspaceSnapshotsOnDemand`, `usageLimitsCommand`; nur Schema untersucht, **nicht ausgeführt**. Explizites Refresh kann Providerprozesse oder Authentifizierung anstoßen.

**Empfehlung:** Dokumentiertes HTTP-MCP plus OAuth als primäre Integrationsgrenze. CLI ist ein offiziell vorhandener, aber eingeschränkter Fallback. HTTP-Snapshots und Effect-WS-Protokoll nur als versionierter Zusatzadapter; kein Stabilitätsversprechen und keine direkte interne Service-Imports. Externe MCP-**Lese**zugriffe sind bewiesen, Schreibzugriffe über die CLI bewiesen. Companion-MCP-Mutationen mit schreibender OAuth-Berechtigung und allen Modelloptionen stehen noch als gezielter Integrationstest aus.

## 3. Status, Antworten und Push

CLI-Show liefert `{thread, messages}`; Thread hat unter anderem `status`, `activeRunId`, `latestRunId`, `pendingRequestCount`, `settled`, `runtimeMode`, `interactionMode`, `worktreePath`. Messages: `{role, runId, text}`. Das letzte Assistant-Element muss zum gesendeten `runId` gehören; eine alte Antwort nach einem neuen Follow-up ist kein Abschlussbeleg.

`start/send --wait --json` liefern am Ende `{threadId, runId, status, reply, error}`. Hilfe: Exit 0 erfolgreich, 1 fehlgeschlagen, 2 wartet auf Antwort. Nur Exit 0 wurde ausgeführt. Der CLI-Code nutzt `t3_thread_wait` in Scheiben und kontrolliert Pending Requests; kein Stream auf stdout während des Wartens. Ohne `--wait` sofortige Receipt; `send` verwendet **queue**, unterbricht laufende Arbeit nicht.

MCP bietet `t3_thread_read` mit `view: messages|activity`, `recentRuns`, `afterPosition/nextPosition`, Textlimits und Trunkierungsmetadaten; `t3_thread_wait` pinnt einen Run. Pending Requests sind genauer zu lesen, bevor `needs_approval` versus Benutzerfrage unterschieden wird. Ein Zähler allein reicht nicht. Approval-/Fehler-/Cancel-Zustände wurden nicht künstlich ausgelöst.

Push ist real vorhanden: Bearer → `POST /api/auth/websocket-ticket/rpc` → `ws://127.0.0.1:3773/ws?orchestrationProtocol=2&wsTicket=<ticket>`. Native Node-26-WebSocket reicht. Request-Frame:

```json
{"_tag":"Request","id":"1","tag":"orchestration.subscribeThread","payload":{"threadId":"mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f"},"headers":[]}
```

Antwort im realen Test (nur Metadaten geloggt):

```json
{"frameTag":"Chunk","keys":["_tag","requestId","values"],"bytes":34026}
```

Ein initialer Snapshot-Chunk ist bewiesen; laufende Zustandswechsel, Reconnect/Sequenz-Replay und Backpressure sind noch nicht abgenommen. `server.getConfig` kam als `Exit` mit `{_tag, requestId, exit}`. Dieses Effect-RPC-Protokoll ist **kein JSON-RPC 2.0** wie MCP.

Fünf separate `thread show --messages 1 --json`-Prozesse: 531, 526, 535, 524, 546 ms; je 1070 stdout-Bytes. Child-CPU zusammen 1.747685 s User + 0.391327 s System; größter Child-RSS 180452 KiB (Python `resource.getrusage(RUSAGE_CHILDREN)`, keine Summe gleichzeitig lebender Prozesse). Das sind lokale Prozesskosten, keine Tokenabrechnung. Polling selbst startet keinen Modellturn. Bei 2 s Pollintervall liegt zusätzliche Erkennungsverzögerung grob zwischen 0 und 2 s plus ca. 0,53 s Abruf; viele parallele CLI-Poller sind ungeeignet. Für MVP wenige aktive Threads alle 3–5 s, gebündelt nach Projekt, pausierte/terminale Threads zurücknehmen. Dauerhafter MCP-Client vermeidet Launcher-Neustarts; Push erst nach Reconnect-Test einsetzen.

## 4. Dateien und Attachments

Keine Datei-/Bildflags in `thread start/send`. Absolute Manifest-/Assetpfade sind der geprüfte CLI-Weg. Der zweite Test beweist Zugriff aus einem neuen T3-Worktree auf `~/.local/share/zuruf/feasibility/probe.txt` unter dem tatsächlich verwendeten `full-access`-Modus. **Nicht auf andere Sandboxes oder Remote-Umgebungen übertragen.** Bildverständnis wurde nicht getestet; ein Textdateilesetest beweist keinen visuellen Bildzugriff.

Das lokale MCP-Schema enthält `t3_thread_launch.attachments` und `t3_thread_send_attachments`. Ein Attachment ist eine Referenz `{type: image|file, id, name, mimeType, sizeBytes, source?}`; kein beliebiger lokaler Pfad oder Base64-Feld. Maximal acht, Bild bis 10 MiB beziehungsweise Datei bis 50 MiB laut Schema. Upload-/Persist-Transport wurde nur im gebündelten API-Schema gefunden (`attachments.createUploadUrl`, `assets.persistChatAttachments`), nicht praktisch getestet. Keine erfundene Attachment-ID senden. P0 absolute Pfade und Lesbarkeitsprüfung, P1 expliziter Attachment-Uploadtest.

## 5. Konkreter T3Adapter

| Methode aus §8.1 | Empfehlung und Nachweisgrenze |
|---|---|
| `discoverCapabilities` | MCP Initialize + `tools/list`; Version und Schema-Fingerprint speichern. CLI `--version/--help` für Fallback. Nicht aus Versionsnummer allein auf Features schließen. |
| `listProjects` | MCP `t3_project_list` mit Cursor/Limit 100; alternativ bewiesenes HTTP `GET /api/projects`. CLI bietet keine Liste. |
| `listProvidersAndModels` | MCP `orchestrator_capabilities`, Suchabfragen und Optionen übernehmen, Disabled/Constraints anzeigen. Optional WS-Katalog für vollständige Liste. |
| `createThread` | MCP `t3_thread_launch` mit Projekt-ID, `modelSelection:{instanceId,model,options}`, `runtimeMode`, `interactionMode`, `workspaceStrategy`, `clientRequestId`; Schema bewiesen, schreibender externer OAuth-Test noch offen. CLI-Fallback nur für unterstützte Parameter; unbekannte Wünsche ablehnen statt still ignorieren. |
| `sendMessage` | MCP `t3_thread_send` mit `mode: queue`, stabiler Request-ID; CLI `thread send --json` als real bewiesener Fallback. |
| `getThreadStatus` | MCP `t3_thread_read`, Run-ID korrelieren, Activity/Pending Requests bei Bedarf ergänzen. CLI Show als Fallback. |
| `getThreadMessages` | Inkrementelles MCP Read einschließlich Pagination/Trunkierung; nur Assistanttext des passenden Runs als Ergebnis übernehmen. |
| `interruptThread` | Nur aktivieren, wenn `t3_thread_interrupt` im Schema angeboten und schreibende Session berechtigt ist; hier nicht ausgeführt. Kein CLI-Unterbefehl dafür. |

Workspace-Mapping: `new_worktree → {type: worktree, baseRef, branch, startFromOrigin:false}`, `existing_worktree → {type: existing_worktree, worktreePath}`, `shared_root → {type: root}`. Benötigte zusätzliche Werte (`baseRef`, Pfad) fehlen noch im sinngemäßen Spec-Interface und müssen konkret ergänzt werden. Nicht manuell im Prompt einen Worktree erstellen: T3 muss ihn **vor** Providerstart binden.

Idempotenz: Batch-UUID und Requestparameter atomar in der lokalen Outbox persistieren. `clientRequestId` nur senden, wenn tatsächliches Schema vorhanden; bei verlorenem Resultat zuerst projektbezogen Titelmarker und Run prüfen. Die ältere Onlinedoku und lokale Launch-Signatur widersprechen sich; vor automatischem Replay muss ein spezieller Replay-Test zeigen, ob der Key über Verbindungs-/Sessionwechsel stabil wirkt. Bis dahin keine behauptete Exactly-once-Garantie; unklarer Ausgang wird `unknown_remote_state`. Bei `send` nennt die Doku stabile Command-/Message-IDs aus Request-ID, ebenfalls vor automatischem Retry testen.

Statusabbildung auf §10:

| Remote/Zustand | Zuruf |
|---|---|
| Lokaler editierbarer Puffer / Transkriptabschluss | `draft` / `finalizing_transcript` |
| Outbox bereit / Request unterwegs | `queued` / `dispatching` |
| `preparing`, `queued` | `queued` (Remote-Detail erhalten) |
| `starting`, `running` | `running` |
| `waiting` oder Pending Request: Approval nachgewiesen | `needs_approval` |
| Benutzerfrage / uneindeutiges Waiting | `needs_review` mit Originalgrund; kein Approval erfinden |
| `completed` des Zielruns | `completed`, Ergebnis nur zu dessen Run; `settled` separat für nachlaufende Arbeit |
| `failed` | `failed`, Fehler aus Activity lesen |
| `cancelled`, `interrupted` | `cancelled`, Originalstatus behalten |
| `rolled_back`, idle ohne gestarteten Run | `needs_review` |
| Timeout, unbekannter Status, verlorene Receipt | `unknown_remote_state`, kein automatischer Neuversand |

## 6. Voxtype-Dateiausgabe und Zustandsmodell

Die vollständigen Startflags stehen in `spikes/voxtype/help.txt`. Unterstützt werden `--type`, `--clipboard`, `--paste`, `--file [FILE]`, `--model`, `--profile`, `--auto-submit/--no-auto-submit`, `--shift-enter-newlines/--no-shift-enter-newlines`, `--no-osd`, `--smart-auto-submit/--no-smart-auto-submit`. Für Zuruf:

```bash
voxtype record start --file=/absolute/unique/segment.txt --no-auto-submit --no-smart-auto-submit --no-osd
voxtype record stop --wait --json --timeout 30
voxtype record cancel
voxtype status --format json --extended
voxtype status --follow --format json
```

`--file=…` ist ein Aufnahme-spezifischer Output-Override; die aktuelle Config bleibt unverändert. Auflösung via `voxtype config` wurde read-only im Prozess gefiltert, keine Secrets ausgegeben: Output `Paste`, Whisper `base.en`, Audio `default`, 16000 Hz, Meetingchunk 30 s. `--file` routete im Test trotz Paste-Default in eine Datei. Die dokumentierte Semantik unterdrückt die normale Typing-/Paste-Ausgabe; eine unabhängige Desktop-Tastaturereignisaufzeichnung wurde nicht durchgeführt. Keine globalen Outputflags mit dem Aufnahme-Override verwechseln.

`status` Text lieferte `idle`; JSON ist Waybar-förmig `{text,alt,class,tooltip}`. Maschinenzustand aus `alt`/`class`, nicht aus Emoji/Tooltip lesen. `--extended` ergänzt `model`, `device`, `backend`. `--follow` lieferte direkt eine Idle-JSON-Zeile und bleibt bis zum Beenden aktiv (kurzer Test beendet). Aufnahme-JSON hatte `alt/class: recording`.

`record start` signalisiert asynchron den bestehenden Daemon; Exit 0 bedeutet keine bestätigte exklusive Aufnahme. Zweiter Start während eigener Aufnahme ergab ebenfalls Exit 0 ohne Ausgabe. Ob dabei intern ignoriert, überschrieben oder zurückgesetzt wird, wurde nicht nachgewiesen. Deshalb niemals „start erfolgreich“ als Besitznachweis behandeln oder damit eine fremde Aufnahme übernehmen.

`stop --wait --json` ist der zuverlässige Finalisierungsvertrag: Hilfe nennt 0 transcribed, 3 nothing to transcribe, 4 timeout, 1 failed. `--wait-file` optional, Standard ist der beim Start übergebene Dateipfad. Nur Exit 0 wurde hier real beobachtet. Datei erst nach Abschluss lesen, Ergebnistext/Datei gegenprüfen, in eigenen immutable Segmentdatensatz übernehmen. Keine vorhandene Zieldatei wiederverwenden; sonst könnte ein alter Inhalt als neues Ergebnis erscheinen. Größenstabilität allein ist kein Abschlussnachweis.

Der stille Versuch erzeugte nach 357 ms Stop-Wartezeit:

```json
{"chars":1,"message":null,"status":"ok","text":"."}
```

Datei: zwei Bytes, Inhalt `.\n`. Empty/Halluzination nicht automatisch als gültigen Auftrag versenden. Der separate Cancel-Versuch brauchte 66 ms, Status danach Idle und Zieldatei `ENOENT`; Abbruch während laufender Transkription wurde nicht separat getestet. Aufnahmen jeweils ungefähr zwei Sekunden, Daemon vorher Idle und am Ende wieder Idle. Kein Neustart, keine Configänderung.

### SpeechAdapter und Shift+N

Zustandsfolge `unavailable → idle → starting → recording → stopping → transcribing → finalized|empty|error`, zusätzlich `cancelled` und `busy_external`. Pro Segment UUID, Epoch-/Batch-ID, monotone Zeitmarker, eigenes restriktives Verzeichnis und noch nicht vorhandener absoluter Dateipfad. Befehle serialisieren, Start durch Statuswechsel bestätigen, Timeout und Fremdänderungen behandeln. Auf Appstart bei fremder Aufnahme nur `busy_external`; nie automatisch Stop/Cancel. Lokaler Lock verhindert andere Zuruf-Prozesse, **nicht** den Voxtype-Hotkey; diese Race Condition muss sichtbar bleiben.

Shift+N schließt den alten Batch samt Segment-ID atomar ab (`finalizing_transcript`), erzeugt sofort neue Epoch/Batches für Captures und UI. SpeechAdapter stoppt die **eigene** Aufnahme mit `--wait --json`, ordnet Ergebnis ausschließlich dem alten Segment zu, prüft Idle und startet eine neue Datei für die neue Epoch. Erst nach finalisiertem Snapshot dispatchen. Fehler lassen den alten Batch korrigierbar und verhindern Vermischung; der neue UI-Puffer bleibt bedienbar, zeigt währenddessen aber ausdrücklich eine Audio-Lücke. Keine neue Aufnahme vor dem nachgewiesenen Abschluss auf denselben Daemon schicken.

Gemessene Untergrenze für diese Stop/Finalize/Restart-Sequenz: 357 ms Finalisieren + etwa 4–5 ms Startkommando + tatsächlicher Audioanlauf/IPC/Scheduling. Das ist **keine gemessene exakte akustische Lücke**. Lange Sprache oder langsames Modell erhöhen die Finalisierungszeit. Auch ein Dispatch im Hintergrund kann diese Daemon-Grenze nicht aufheben. P0 muss sie anzeigen; Spezifikationsaussagen zur Unterbrechungsfreiheit bleiben damit eine UX-Anforderung, kein erbrachter Audio-Nachweis.

## 7. Meeting und noch offene Sprachtests

`voxtype meeting` ist in dieser Installation vorhanden: `start`, `stop`, `pause`, `resume`, `status`, `list`, `export`, `show`, `delete`, `label`, `summarize`. `meeting status` meldete `No meeting currently in progress.` Start bietet Titel und `--diarization simple|ml`. Export: `--format text|markdown|json`, `--output`, `--timestamps`, `--speakers`, `--metadata`. Hilfe beschreibt kontinuierliche Chunkverarbeitung; Configchunk beträgt 30 s, Mikrofon plus automatischer Loopback. Kein Meeting gestartet, damit keine parallele Mikrofon-/Systemaudioaufnahme und kein zusätzlicher schwerer Prozess entsteht.

Damit ist P1 **plausibel, nicht abgenommen**: JSON-Segmentform, tatsächliche Zeitauflösung, Live-Lesbarkeit vor Export, Restchunk-Finalisierung, Zuordnung an einer Grenze innerhalb eines 30-s-Chunks, Modell-/Ressourcenkosten und Koexistenz mit Diktat sind offen. Export-Timestamps allein beweisen keine wortgenaue Shift+N-Trennung. Daueraufnahme könnte die Stoplücke vermeiden, benötigt aber tatsächliche Segmentzeit-/Audiogrenzen statt heuristischer Textteilung. Keine automatische Nutzung von `meeting summarize` oder externen Summary-Diensten.

Keine TTS-Tools gefunden: `espeak`, `espeak-ng`, `pico2wave`, `flite`, `piper`, `spd-say`, `say`, `mimic3`, `RHVoice-test`; ffmpeg ist vorhanden. Deshalb kein Sprach-WAV erzeugt und `voxtype transcribe <WAV>` nur per Hilfe bestätigt (16 kHz, mono; `--engine` Override). Ein zweiter ASR-Prozess wurde nicht gestartet. Erforderlicher Nutzerabnahmetest: deutsches gesprochenes Diktat, drei Capturegrenzen, Shift+N beim Sprechen, leere Aufnahme, Cancel während Transkription, Daemonfehler/Timeout. Das aktuell konfigurierte `.en`-Modell ist für deutsche Qualität besonders zu prüfen; kein automatischer Modellwechsel.

## 8. Fakten, Annahmen und offene Freigabe für Implementierung

**Verifiziert:** Versionen und Flagoberflächen; externer CLI-Threadstart/Follow-up mit explizitem Modell; Root-/Worktreebindung; absolute Textdatei aus Worktree lesbar; echte Modell-/Optionen-Discovery; OAuth-PKCE-MCP read-only ohne Parentthread; allgemeiner Bearer für HTTP/WS; initialer Push-Chunk; Voxtype-Dateirouting, Stop-Wait, Cancel, Busy-Start-Erfolg und Stille-Halluzination; Pollkosten.

**Nur aus aktuellem Schema/Quellen abgeleitet:** frei wählbare MCP-Launchoptionen, Existing-Worktreebindung, Attachments nach Upload, Unterbrechung, Fehler-/Approvalsemantik und kontinuierliche Meetingsegmente. Diese Optionen nicht als End-to-End getestet ausweisen.

**Blocker für vollständige Abnahme:** Launch-/Send-Replay über Verbindungs-/Credentialwechsel; Companion-MCP-Mutation mit gezielt begrenzter Schreibberechtigung; tatsächlicher Bild-/Uploadtest; Approval-/Frage-/Fehleranzeige; Push-Reconnect; echte deutsche Sprache und Audiogrenzen. **Kein grundsätzlicher P0-Verbindungsblocker** mehr. Gleichzeitige exklusive Voxtype-Nutzung durch Zuruf und Hotkey ist technisch nicht durch Idle-Polling garantiert. Lückenlose Sprache bleibt P1-Forschung.

## 9. Unveränderte Belege der realen Versuche

Die folgenden Ausgaben stammen aus den Prüfprozessen. JSON wurde zur Lesbarkeit eingerückt; Inhalte und Feldwerte sind unverändert. Keine Geheimnisse enthalten. Die beiden erstellten Threads und der T3-Worktree bleiben für Sichtprüfung erhalten; keine weiteren Testthreads erzeugt.

```text
$ t3 thread start --title zuruf-feasibility-2026-11-10T16-39-39.803Z --provider codex --model gpt-6-luna --json /home/mika/Projects/Humaify/zuruf/main 'Reply with the single word OK. Do not modify any files.'
{
  "threadId": "mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f",
  "projectId": "mcp:54cd96c7-4e02-4096-a57d-dd0a10441b9c",
  "runId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1"
}
```

Messdauer: 594 ms.

```text
$ t3 thread show --json --messages 3 mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f
{
  "thread": {
    "threadId": "mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f",
    "projectId": "mcp:54cd96c7-4e02-4096-a57d-dd0a10441b9c",
    "title": "zuruf-feasibility-2026-11-10T16-39-39.803Z",
    "link": "https://app.t3.codes/threads/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f",
    "createdBy": "agent",
    "creationSource": "mcp",
    "status": "running",
    "latestRunId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
    "activeRunId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
    "providerInstanceId": "codex",
    "model": "gpt-6-luna",
    "runtimeMode": "full-access",
    "interactionMode": "default",
    "linkedPullRequest": null,
    "pullRequests": [],
    "titleRegeneration": null,
    "branch": null,
    "worktreePath": null,
    "parentThreadId": null,
    "relationshipToParent": null,
    "runCount": 1,
    "itemCount": 2,
    "pendingRequestCount": 0,
    "archived": false,
    "settled": false,
    "settledAt": null,
    "snoozed": false,
    "snoozedUntil": null,
    "createdAt": "2026-11-10T16:39:40.320Z",
    "updatedAt": "2026-11-10T16:39:40.613Z"
  },
  "messages": [
    {
      "role": "user",
      "runId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
      "text": "Reply with the single word OK. Do not modify any files."
    }
  ]
}
```

Messdauer: 541 ms.

```text
$ t3 thread show --json --messages 3 mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f
{
  "thread": {
    "threadId": "mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f",
    "projectId": "mcp:54cd96c7-4e02-4096-a57d-dd0a10441b9c",
    "title": "zuruf-feasibility-2026-11-10T16-39-39.803Z",
    "link": "https://app.t3.codes/threads/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f",
    "createdBy": "agent",
    "creationSource": "mcp",
    "status": "completed",
    "latestRunId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
    "activeRunId": null,
    "providerInstanceId": "codex",
    "model": "gpt-6-luna",
    "runtimeMode": "full-access",
    "interactionMode": "default",
    "linkedPullRequest": null,
    "pullRequests": [],
    "titleRegeneration": null,
    "branch": null,
    "worktreePath": null,
    "parentThreadId": null,
    "relationshipToParent": null,
    "runCount": 1,
    "itemCount": 4,
    "pendingRequestCount": 0,
    "archived": false,
    "settled": false,
    "settledAt": null,
    "snoozed": false,
    "snoozedUntil": null,
    "createdAt": "2026-11-10T16:39:40.320Z",
    "updatedAt": "2026-11-10T16:39:43.113Z"
  },
  "messages": [
    {
      "role": "user",
      "runId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
      "text": "Reply with the single word OK. Do not modify any files."
    },
    {
      "role": "assistant",
      "runId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:1",
      "text": "OK"
    }
  ]
}
```

Messdauer: 529 ms.

```text
$ t3 thread send --wait --json mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f 'Reply with the single word OK. Do not modify any files.'
{
  "threadId": "mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f",
  "runId": "run:thread:mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f:ordinal:2",
  "status": "completed",
  "reply": "OK",
  "error": null
}
```

Messdauer: 3430 ms.

```text
$ voxtype status
idle
```

Messdauer: 5 ms, Exit 0.

```text
$ voxtype record start --file=/tmp/zuruf-voxtype-fMhRoO/transcript.txt --no-auto-submit --no-smart-auto-submit --no-osd
```

Messdauer: 4 ms, Exit 0.

```text
$ voxtype status --format json --extended
{
  "text": "🎤",
  "alt": "recording",
  "class": "recording",
  "tooltip": "Recording...\nModel: base.en\nDevice: default\nBackend: CPU (AVX-512)",
  "model": "base.en",
  "device": "default",
  "backend": "CPU (AVX-512)"
}
```

Messdauer: 343 ms, Exit 0.

```text
$ voxtype record start --file=/tmp/zuruf-voxtype-fMhRoO/transcript.txt --no-auto-submit --no-smart-auto-submit --no-osd
```

Messdauer: 4 ms, Exit 0.

```text
$ voxtype record stop --wait --json --timeout 30
{
  "chars": 1,
  "message": null,
  "status": "ok",
  "text": "."
}
```

Messdauer: 357 ms, Exit 0.

```text
$ voxtype status --format json
{
  "text": "🎙️",
  "alt": "idle",
  "class": "idle",
  "tooltip": "Voxtype ready - hold hotkey to record"
}
```

Messdauer: 4 ms, Exit 0.

```json
{
  "path": "/tmp/zuruf-voxtype-fMhRoO/transcript.txt",
  "size": 2,
  "text": ".\n"
}
```

```text
$ voxtype status
idle
```

Messdauer: 5 ms, Exit 0.

```text
$ voxtype record start --file=/tmp/zuruf-voxtype-mA4PgM/transcript.txt --no-auto-submit --no-smart-auto-submit --no-osd
```

Messdauer: 5 ms, Exit 0.

```text
$ voxtype status --format json --extended
{
  "text": "🎤",
  "alt": "recording",
  "class": "recording",
  "tooltip": "Recording...\nModel: base.en\nDevice: default\nBackend: CPU (AVX-512)",
  "model": "base.en",
  "device": "default",
  "backend": "CPU (AVX-512)"
}
```

Messdauer: 343 ms, Exit 0.

```text
$ voxtype record cancel
```

Messdauer: 66 ms, Exit 0.

```text
$ voxtype status --format json
{
  "text": "🎙️",
  "alt": "idle",
  "class": "idle",
  "tooltip": "Voxtype ready - hold hotkey to record"
}
```

Messdauer: 5 ms, Exit 0.

```json
{
  "path": "/tmp/zuruf-voxtype-mA4PgM/transcript.txt",
  "fileError": "ENOENT"
}
```

Worktree-Nachweis:

```text
$ t3 thread start --title zuruf-feasibility-worktree-20261110 --provider codex --model gpt-6-luna --worktree HEAD --branch zuruf-feasibility-worktree-20261110 --json /home/mika/Projects/Humaify/zuruf/main "Read /home/mika/.local/share/zuruf/feasibility/probe.txt and reply with its exact contents and your working directory. Do not modify any files."
{"threadId":"mcp:0c30c012-8acf-47f8-9695-1224fb824a4d","projectId":"mcp:54cd96c7-4e02-4096-a57d-dd0a10441b9c","runId":"run:thread:mcp%3A0c30c012-8acf-47f8-9695-1224fb824a4d:ordinal:1"}
```

```text
$ t3 thread show --json --messages 3 mcp:0c30c012-8acf-47f8-9695-1224fb824a4d
{
  "thread": {
    "threadId": "mcp:0c30c012-8acf-47f8-9695-1224fb824a4d",
    "projectId": "mcp:54cd96c7-4e02-4096-a57d-dd0a10441b9c",
    "title": "zuruf-feasibility-worktree-20261110",
    "link": "https://app.t3.codes/threads/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp%3A0c30c012-8acf-47f8-9695-1224fb824a4d",
    "createdBy": "agent",
    "creationSource": "mcp",
    "status": "completed",
    "latestRunId": "run:thread:mcp%3A0c30c012-8acf-47f8-9695-1224fb824a4d:ordinal:1",
    "activeRunId": null,
    "providerInstanceId": "codex",
    "model": "gpt-6-luna",
    "runtimeMode": "full-access",
    "interactionMode": "default",
    "linkedPullRequest": null,
    "pullRequests": [],
    "titleRegeneration": null,
    "branch": "zuruf-feasibility-worktree-20261110",
    "worktreePath": "/home/mika/.t3/worktrees/main/zuruf-feasibility-worktree-20261110",
    "parentThreadId": null,
    "relationshipToParent": null,
    "runCount": 1,
    "itemCount": 5,
    "pendingRequestCount": 0,
    "archived": false,
    "settled": true,
    "settledAt": "2026-11-10T16:42:43.057Z",
    "snoozed": false,
    "snoozedUntil": null,
    "createdAt": "2026-11-10T16:42:25.826Z",
    "updatedAt": "2026-11-10T16:42:43.057Z"
  },
  "messages": [
    {
      "role": "user",
      "runId": "run:thread:mcp%3A0c30c012-8acf-47f8-9695-1224fb824a4d:ordinal:1",
      "text": "Read /home/mika/.local/share/zuruf/feasibility/probe.txt and reply with its exact contents and your working directory. Do not modify any files."
    },
    {
      "role": "assistant",
      "runId": "run:thread:mcp%3A0c30c012-8acf-47f8-9695-1224fb824a4d:ordinal:1",
      "text": "ZURUF_ASSET_PROOF_20261110\n/home/mika/.t3/worktrees/main/zuruf-feasibility-worktree-20261110"
    }
  ]
}
```
