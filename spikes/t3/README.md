# T3-Spike

Ausführen mit Node 26, ohne Dependencies, aus diesem Repository. `T3_URL` überschreibt den im Spike beobachteten Loopback-Port 3773. Produktcode muss die Origin aus `~/.t3/userdata/server-runtime.json` ermitteln; keine Secrets dort ausgeben.

- `node spikes/t3/mcp-discovery.mjs`: registriert einen OAuth-Client, paart read-only über einen kurzlebigen CLI-Code und fragt reale Tool-Schemas, Modelle und den vorhandenen Testthread ab. Secrets nur im Prozessspeicher. Neue MCP-Sessions werden im `finally` über die CLI widerrufen. OAuth-Zugänge leben sonst 30 Tage, unabhängig vom 5-Minuten-Pairing-Code. Parallele gleichnamige Spike-Läufe vermeiden.
- `node spikes/t3/http-ws-discovery.mjs`: kurzlebigen Bearer ausstellen, Zuruf-Projekt per HTTP lesen, WebSocket-Ticket einlösen, `server.getConfig` aufrufen, nur Provider-/Modellmetadaten ausgeben. Session im `finally` widerrufen.
- `RPC_METHOD=orchestration.subscribeThread node spikes/t3/http-ws-discovery.mjs`: initialen Push-Chunk des vorhandenen Testthreads nachweisen, Verbindung schließen.
- `node spikes/t3/thread-proof.mjs`: **erstellt einen echten Thread** mit kleinem Modell und liest einmal Status. Ausgabe dauerhaft sichern. Danach ausschließlich `node spikes/t3/thread-proof.mjs <threadId>` wiederholen. `SEND_FOLLOWUP=1` sendet genau bei diesem Aufruf ein weiteres echtes Follow-up und wartet auf dessen Ergebnis. Kein automatischer Wiederholungsversand.
- `node spikes/t3/worktree-proof.mjs --create`: **erstellt einen weiteren echten Thread/Worktree** und eine harmlose Textdatei unter `~/.local/share/zuruf/feasibility/`. Anschließend Status mit dem vorigen Script abrufen. Nicht automatisch erneut ausführen.

Die vorhandenen Testthread-IDs stehen im Bericht. `help.txt` ist die unveränderte lokale CLI-Hilfe. Diese Spikes sind Prüfprogramme, keine produktionsfertigen Adapter. CLI-Timeout ist kein Remote-Abbruch; nie blind neu erstellen. Keine Server werden gestartet, T3 und Datenbank bleiben unverändert.
