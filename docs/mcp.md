# MCP-Server und Übergabe an die Verdrahtung

`zuruf-mcp` implementiert den kleinen JSON-RPC-Vertrag ohne MCP-Runtime-Abhängigkeit. Der Daemon mountet `/mcp` auf seinem bestehenden Listener (127.0.0.1). Unterstützt sind Streamable HTTP mit JSON-Antworten, `initialize`, `notifications/initialized`, `ping`, `tools/list` und `tools/call`, Protokollversionen 2025-03-26, 2025-06-18 und 2025-11-25. Kein SSE-Abonnement: GET liefert 405. Die Revision 2026-07-28 ist nicht implementiert. Grundlage für die unterstützte Transportversion: [offizielle MCP-Spezifikation](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).

Jeder Request braucht `Origin: http://127.0.0.1:<port>` und `Authorization: Bearer <agent credential>`. Das UI-Token wird auf `/mcp` abgewiesen. Tokens sind zufällige 256-Bit-Werte, flüchtig im Daemon, maximal 24 Stunden gültig (Daemon-Übergabe: eine Stunde), einzeln je Task widerrufbar. Neustart widerruft alle. Nach jedem Toolaufruf wird die Gültigkeit erneut geprüft. Ausgabe enthält nur das Projekt, den Batch und die Session des zugewiesenen Tasks; `list_tasks` zeigt ausschließlich diesen Task.

## Übergabevertrag für Dispatch und CDP

1. Nach Anlegen des Tasks `Daemon::mcp_handoff(Scope { project_id, task_id, session_id }, port).await` aufrufen. Die Funktion prüft Referenzen und liefert `Handoff { url, origin, token }`. Der unabhängige Vertrag ist `AgentAuth::issue` in `zuruf-mcp`.
2. URL und benötigte Headernamen im Dispatch-Prompt nennen: „Review-Kontext über den für diesen Task bereitgestellten Zuruf-MCP-Server abrufen; Agentenzugang aus dem privaten Laufzeitkanal verwenden.“ `batch.json` darf URL/Scope und einen logischen Credential-Verweis enthalten, niemals den Tokenwert.
3. `Handoff.token` ausschließlich über eine private, pro Thread/Run isolierte MCP-Client-Konfiguration oder Credential-Injection übergeben. Nicht als Prompttext, CLI-Argument, Log oder versionierte Datei. Wenn T3 keinen solchen externen Credential-Kanal unterstützt, bleibt die automatische Übergabe **offen**. Es wurde kein T3-Endpunkt erfunden und keine T3-Datei geändert. Eine private Datei müsste außerhalb des Repositories mit 0600 liegen und nach dem Run entfernt werden; gemeinsames globales Provider-Environment ist nicht taskisoliert.
4. CDP muss **dieselbe** Registry über `Daemon::mcp_agent_scope(token)` verwenden. Scope und Session vor jedem Befehl prüfen; beim Browser-Arbiter zusätzlich Task-ID als `agent_id`, aktuelle Lease und menschliches Takeover prüfen. Niemals einen zweiten unverbundenen Agententoken erzeugen oder den rohen Chromium-Port ausgeben. `get_cdp_endpoint` liefert ausschließlich `/cdp/<sessionId>` und nur bei aktueller Lease. Endpointkenntnis ersetzt keine Autorisierung.
5. Abbruch, Abschluss oder explizites Trennen müssen `Daemon::revoke_mcp_task(task_id)` aufrufen und Browserkontrolle widerrufen. Diese Aufrufe sind **noch vom Wiring-Agenten einzubauen**. Keine Änderungen an Dispatch in diesem Branch.

Für lokale Controller und Integrationstests existiert zusätzlich `POST /api/v1/mcp/credentials` mit UI-Bearer und exakt lokalem Origin, Body `{scope:{projectId,taskId,sessionId},revoke?:boolean}`. Antworten sind `Cache-Control: no-store`. Dieser Endpunkt gehört nicht zur Agententoolliste. Den Responsebody niemals protokollieren.

## Werkzeuge

| Werkzeug | Wirkung |
|---|---|
| list_projects | Zugewiesenes Projekt mit redigierter Start-URL |
| list_sessions | Zugewiesene Session, Modus, Tabs und Kontrolle |
| get_session_context | Aktueller Sessionkontext wie oben |
| list_reviews | Review des Tasks |
| list_annotations | Annotationen des Task-Batches; optionale Batch-/Session-/Statusfilter |
| get_annotation | Redigierte Annotation mit DOM/Locatoren und Asset-IDs |
| get_annotation_asset | Autorisierte PNG-Datei als MCP-Bildinhalt (maximal 20 MiB) |
| get_batch_manifest | Versionierter Kontext, Transkripte und Assetreferenzen |
| update_annotation_status | Status open/in_progress/done/wont_fix persistieren |
| list_tasks | Nur den zugewiesenen Task anzeigen |
| report_progress | Nachricht persistieren, optional needs_review setzen |
| request_browser_control | Lease für den Task anfordern (1–300 Sekunden) |
| renew_browser_control | Derzeit expliziter Toolfehler, siehe Vertragslücke |
| release_browser_control | Eigene Lease freigeben |
| get_cdp_endpoint | Gefilterten CDP-Endpunkt bei eigener Lease abrufen |

Die elf Werkzeuge aus Spec §9 bleiben erhalten; Taskmeldungen, Lease-Erneuerung und Endpointabruf ergänzen den beauftragten Umfang. Architektur hat Vorrang bei Rust, Datenablage und Authentifizierung. Der Protocol-v1-Vertrag wird nicht geändert. `get_annotation` nutzt dieselbe URL-/DOM-Redaktion wie `build_manifest`; Screenshots und Freitext werden nicht automatisch inhaltlich anonymisiert und benötigen Sichtprüfung.

## Grenzen

`BrowserController::control` kennt nur grant/release/takeover/revoke, keine atomare Verlängerung. Ein Release mit anschließendem Grant könnte ein menschliches Takeover überholen. Deshalb schlägt Erneuern geschlossen fehl. Die Browser-Verdrahtung muss eine atomare Renew-Operation ergänzen; andere Crates wurden hier nicht geändert. Die Tests prüfen diese ehrliche Fehlermeldung, keine erfolgreiche Verlängerung.

Die Daemon-Übergabe akzeptiert aktuell nur Batches, deren sessiongebundene Annotationen derselben Session zugeordnet sind. Reine Textnotizen ohne Session-ID bleiben im Task sichtbar. Gemischte Batches mit mehreren Sessions werden sicherheitshalber abgewiesen; eine Übergabe mit mehreren Session-Scopes fehlt noch.

`get_session_context` liefert vorhandene Tabs und Kontrolle; der Browservertrag besitzt keinen aktuellen Viewport außerhalb der gespeicherten Annotationen. Der liegt im Batch-Kontext. Fortschrittsstatus kann von der bestehenden Remote-Statusverfolgung wieder aktualisiert werden. Das Beenden laufender CDP-Kommandos und deren unmittelbare Sperre nach Takeover bleiben Aufgabe des CDP-Proxys.

## Reproduzieren

Alle Befehle aus dem Worktree und speicherbegrenzt ausführen:

```sh
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo test --workspace -j 2
systemd-run --user --wait --collect --pipe --working-directory="$PWD/tests/mcp" -p MemoryMax=1G -- npm ci --no-audit --no-fund
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- node tests/mcp/client.mjs
```

Der offizielle TS-SDK-Client startet das echte Fake-Daemon-Binary mit Seed, separaten Verzeichnissen und Port 48673. Er prüft alle 15 Werkzeuge, MCP-Bildinhalt, Auth/Origin, gegenseitige Task-Abgrenzung, Widerruf, Lease/Freigabe, Ablauf einer Ein-Sekunden-Lease und Takeover. Prozess und temporäre Daten werden im finally-Block entfernt. Ein bestehender Dienst auf dem Testport ist vor dem Test auszuschließen. Reale Chromium-/T3-/Audio-Abnahme wird damit nicht behauptet.

## Stand der lokalen Prüfung

Rust-Workspace-Tests, Clippy (`--workspace --all-targets`, `-D warnings`), Formatprüfung, UI-/Extension-Build und Release-Build wurden erfolgreich ausgeführt. Der offizielle SDK-Test bestand mit zwei getrennten Task-Credentials. Der Installations-Rundlauf bestand einschließlich zweifacher Installation, `systemd-analyze --user verify`, `desktop-file-validate`, Datenerhalt und bestätigtem Purge. `zuruf doctor` meldete auf diesem Rechner Chromium 155, Voxtype 1.1.1, T3 0.1.7, erreichbaren T3-Port sowie vorhandene Hyprland-/PipeWire-Sockets und zwei Modelldateien. Diese Befunde bestätigen keine echte Browser-/Audio-/T3-Integration.

Die wenigen Ergänzungen in `lib.rs` sind Moduldeklaration, AgentAuth-Feld samt Initialisierung, gemeinsame Kontrollsperre samt Initialisierung/Guard und Router-Merge. Alle MCP-Operationen liegen in der neuen `mcp_mount.rs`; die Sperre serialisiert MCP-Kontrollprüfung/-mutation mit dem vorhandenen menschlichen Kontrollpfad. Direkte Browserereignisse müssen weiterhin im Browser-Arbiter selbst korrekt priorisiert werden.
