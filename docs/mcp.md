# MCP-Server und Agentenübergabe

Der Daemon mountet `zuruf-mcp` auf `/mcp` am bestehenden Loopbacklistener. Unterstützt: Streamable HTTP mit JSON-Antworten, `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`; Versionen 2025-03-26, 2025-06-18 und 2025-11-25. GET liefert 405; kein SSE-Abo. Requests benötigen exakt lokalen Origin und ein taskgebundenes Agenten-Bearer. UI-Bearer werden abgewiesen. Antworten enthalten ausschließlich den zugewiesenen Task/Batch/Projekt-/Sessionkontext.

## Verifizierte externe T3-Schnittstelle

Geprüft wurden lokal `t3 thread start --help` und das externe Schema von `t3_thread_launch`: Projekt, Titel, Nachricht, Attachments, Modellwahl, Modi, Workspace und Replay-ID sind vorhanden. **Keine private MCP-/Environment-Konfiguration pro Thread.** Dafür wurde kein T3-Endpunkt erfunden und keine Providerkonfiguration verändert.

Dispatch ruft `Daemon::mcp_handoff(Scope {project_id, task_id, session_id}, port)` auf und schreibt `{url, origin, token}` atomar unter `<state>/agents/<task>/credential.json` mit 0600. Das Zustandsverzeichnis ist privat (0700). Der Prompt nennt ausschließlich den absoluten Dateipfad und die Anweisung, den Inhalt weder auszugeben noch zu kopieren. Manifest, Repository und Logs enthalten keinen Token. Der Agent muss seinen HTTP-/MCP-Client anhand der Datei selbst initialisieren. Dies ist ein überprüfbarer Dateivertrag, keine automatische Toolregistrierung in T3. Prozesse desselben Unix-Nutzers können die Datei lesen: keine OS-Isolation, kein Schutz vor absichtlich bösartigen lokalen Prozessen.

Der Agent liest `url`, setzt `Origin: <origin>`, `Authorization: Bearer <token>`, `Content-Type: application/json` und sendet MCP-JSON-RPC. Nach `initialize`/`notifications/initialized` kann er die Tools nutzen. HTTP-Antworten und insbesondere Bildinhalte nicht vollständig in Logs ausgeben. Freigaben für lokale Datei-/Netzwerkbefehle erteilt ausschließlich der Nutzer in T3.

## Autorisierung und Kontrolle

`/cdp/<sessionId>` validiert MCP-Agenten gegen **dieselbe Registry** (`mcp_agent_scope`), prüft Session und vor jedem Proxybefehl weiter die Registry, Agent-ID und aktive Browserlease. Ein vorhandener manueller Controllerzugang bleibt über die separate explizite Controllerroute verfügbar; er wird nicht als MCP-Taskzugang ausgegeben.

`request_browser_control`, `renew_browser_control`, `release_browser_control` und `get_cdp_endpoint` verwenden den Browser-Arbiter. `ControlRequest.action=renew` verlängert nur eine noch gültige Lease desselben Agenten atomar unter der Takeover-Sperre. Kein Release/Grant-Zwischenfenster. Der Browser sendet bei Ablauf selbst `ControlChanged`; die UI erhält `session.upserted`. Ablauf wird monoton geprüft; Darstellung kann bis etwa 50 ms nachlaufen. Bereits gestartetes JavaScript kann nach Takeover weiterwirken.

Taskabschluss, Remote-Interrupt, Cancel und Shutdown rufen `revoke_mcp_task` auf; Abschluss/Cancel geben auch vorhandene Taskkontrolle zurück. Die Credentialdatei wird entfernt. Neustart verwirft alle alten Dateien und Tokens. Laufende Receipts werden weiter beobachtet; automatische Credentialerneuerung eines schon laufenden Agenten nach Daemonrestart bleibt offen. Nicht sichtbare Remotezustände führen zu `unknown_remote_state`; keine blinde Neusendung.

Die Übergabe unterstützt einen Session-Scope. Gemischte Mehrsessionbatches werden abgewiesen. Reine Textbatches ohne Session bekommen keinen Browserzugang.

## Werkzeuge und lokale Tests

15 Tools: `list_projects`, `list_sessions`, `get_session_context`, `list_reviews`, `list_annotations`, `get_annotation`, `get_annotation_asset`, `get_batch_manifest`, `update_annotation_status`, `list_tasks`, `report_progress`, `request_browser_control`, `renew_browser_control`, `release_browser_control`, `get_cdp_endpoint`.

`POST /api/v1/mcp/credentials` ist ein authentifizierter lokaler Controllerendpunkt zum Ausstellen/Widerrufen; Antwort `no-store`. Credentials niemals protokollieren. `get_annotation_asset` liefert autorisierten PNG-Bildinhalt (maximal 20 MiB).

`tests/e2e/run.sh` prüft Workspace, offiziellen MCP-TS-SDK, headed Browser, Captures, App und Installation jeweils speicherbegrenzt. Der SDK-Test prüft alle 15 Tools einschließlich atomarer Erneuerung, Bildinhalt, Scope, Origin, Ablauf, Widerruf und Takeover. Der zusätzliche Rust-Test `dispatch_credentials_are_private_and_revoked_on_cancel` prüft Dispatchdatei, Modus 0600 und Widerruf. Reale Agentenfreigabe und Bildabruf bleiben gemäß [abnahme.md](abnahme.md) manuell offen.
