# Zuruf-Kern: Start und Verdrahtung

Die drei Crates `zuruf-core`, `zuruf-store` und `zuruf` sind auf `feat/core` implementiert. Der TypeScript-Vertrag wurde nicht geändert. Realer Browser, T3, Voxtype und Desktop-Anbindung bleiben bei den jeweiligen Adapter-Crates.

## Start

Im Checkout `/home/mika/Projects/Humaify/zuruf/worktrees/core`:

```sh
systemd-run --user --wait --collect --pipe \
  -p WorkingDirectory=/home/mika/Projects/Humaify/zuruf/worktrees/core \
  -p MemoryMax=3G -p MemorySwapMax=1G -- \
  mise exec rust@stable -- cargo build -j 2

ZURUF_FAKE_ADAPTERS=1 ./target/debug/zuruf daemon --seed-demo
```

Port: `47600`, ausschließlich `127.0.0.1`; alternativ `ZURUF_PORT`. `zuruf status`, `zuruf action review.toggle`, `zuruf open` sprechen mit diesem Daemon. `open` druckt einen kurzlebigen Einmalcode-Link, niemals das Bearer-Token. SIGINT/SIGTERM beendet Worker und ruft Speech-Cancel, Hotkey-Clear und Session-Stop auf.

`ZURUF_DATA_DIR`, `ZURUF_STATE_DIR`, `ZURUF_CONFIG_DIR` überschreiben alle Datenpfade. Tests nutzen ausschließlich temporäre Verzeichnisse. Die UI wird beim Rust-Build aus `apps/ui/dist` kopiert und auch im Debug-Build eingebettet; ein fehlendes/leeres Verzeichnis liefert eine kleine Platzhalterseite.

## Adaptervertrag

`crates/zuruf-core/src/adapters.rs` enthält objektfähige Traits und ausschließlich typisierte Ergebnisse/Fehler. `AdapterFuture` ist ein `Send` Box-Future. Einstieg: `Daemon::open(paths, Arc<dyn T3Adapter>, Arc<dyn SpeechAdapter>, Arc<dyn BrowserController>, Arc<dyn DesktopHotkeys>)`, anschließend `recover()`, `router()`, zum Beenden `close().await`.

- `create_thread`: Client-Request-ID und Titelmarker sind Task-ID; vollständiges eingefrorenes Profil und Projekt sind Teil der Eingabe. `Ambiguous` bedeutet: Request könnte remote angekommen sein. Vor einem weiteren Launch wird projektbezogen nach dem Marker gesucht. Nur exakt ein Treffer wird übernommen. Auch ein zukünftiger Adapter muss Transportabbrüche nach Versand als mehrdeutig einstufen.
- `thread_status`: Antwort enthält die tatsächliche Run-ID. Nachrichten anderer Runs werden abgewiesen. Ein leerer angefragter Run kommt ausschließlich aus manueller `adopt_thread`-Auflösung; dann muss der Adapter den aktiven/letzten Run ermitteln. Receipts inklusive Run-ID liegen intern in SQLite.
- Speech-Segmente tragen ihre ursprüngliche Batch-/Annotation-ID im Handle. Das Future liefert nur Text. Stop/Start an einer Dispatch-Grenze laufen serialisiert im Hintergrund; ein persistierter Guard blockiert den alten Task bis zur Übergabe der Handles. Marker liefern Segmente der vorherigen Grenze. Transkriptdateien und Originaltext bleiben von Annotation-Edits getrennt.
- Browser-Captures liefern Bildbytes und Seitenkontext. Der Daemon bindet die Annotation vor dem Await an ihren Batch. Der Browser-Adapter verantwortet Picker, Ring-/Crop-Geometrie, In-flight-Befehle, Lease-Timer und das maximal 500 ms lange Auslaufen bei Takeover. `ControlArbiter` stellt dafür reine, monotone Logik bereit.
- SQLite und Datei-IO laufen ausschließlich in `spawn_blocking`. Jede bestätigte Mutation ist vorher committet; Ereignisse werden anschließend veröffentlicht. Metadaten liegen als typisierte JSON-Entitäten mit Referenztabelle und Zeitstempeln in SQLite. Vier eingebettete SQL-Migrationen, WAL, Foreign Keys und Unique-Indizes sichern Referenzen, einen Draft je Review und einen Task je Batch.
- Fake-T3 kann über `FakeT3::set_mode` zwischen Normal, Verbindungsfehler, Timeout vor/nach Launch, Approval und Benutzerfrage wechseln. Die katalogisierten Demo-Provider sind ausdrücklich Fakes. Fake-Remote-Threads existieren nur im Speicher.

## Vertragslücken und verbleibende Grenzen

1. Der Vertrag definiert keinen HTTP-Endpunkt zum Minten des Codes für die CLI. Ergänzt wurde **authentifiziertes `POST /api/v1/auth/code`**. Austausch bleibt `POST /api/v1/auth/exchange`. `/events` ist zusätzlich Alias für `/api/v1/events`.
2. `Task` und `ResolveRequest` enthalten keine Run-ID. Sie wird intern gespeichert; manuelles Adoptieren nutzt die oben beschriebene Leerstring-Konvention. `Routes = never` besitzt keinen serialisierbaren Wert; alle anderen exportierten Protokolltypen haben Rust-JSON-Fixtures.
3. Explizite Task-Follow-ups werden erst nach einem terminalen Run angenommen. Batch-Follow-ups können denselben Thread verwenden. Ein mehrdeutiger Send-Ausgang bleibt sichtbar und wird nicht automatisch wiederholt. Ein generischer Marker identifiziert einen Thread, aber keinen einzelnen Follow-up-Run.
4. Sealed-Batches und deren Annotationen sind nach außen unveränderlich. Späte Transkripte/Captures aus bereits gebundenen Operationen dürfen vervollständigen. Der Vertrag hat keinen Revisions-Endpunkt; spätere fachliche Änderungen benötigen neue Annotationen/Follow-ups. Eine persistierte Originalannotation wird nicht still umgeschrieben.
5. Nach einem Prozessabbruch können verlorene Transkript-Futures nicht rekonstruiert werden: betroffene Segmente werden sichtbar `failed`, alte Tasks werden nicht mit unvollständigem Text versendet. Eine Capture-Platzhalterannotation ohne fertig persistiertes Bild bleibt erhalten und blockiert Dispatch bis zum Fehler-Timeout (120 s). Es gibt noch keinen automatischen Wiederaufnahmepfad für unbestätigte, halb geschriebene Capture-Operationen.
6. Fake-Remote-Zustand überlebt einen Prozessneustart nicht. Lokale Projekte, Drafts, Annotationen, Assets, Receipts und Thread-IDs bleiben erhalten; verschwundene Remote-Threads führen zu `unknown_remote_state`. Mit demselben erreichbaren Adapter wird ein laufender Run nach Neustart weiterverfolgt (Integrationstest).
7. `/ext`, `/cdp`, `/mcp`, sessiongebundene Extension-/Agenten-Tokens, Chromium-/Voxtype-/T3-/Hyprland-Verdrahtung sowie App-Fensterstart gehören zur nächsten Verdrahtungswelle. Die ausführbare CLI verweigert ohne Fake-Flag den Start, bis diese Adapter verdrahtet sind. `open` startet derzeit keinen fehlenden Daemon. Ein Extension-Ursprung kann explizit über `ZURUF_EXTENSION_ORIGIN=chrome-extension://…` zugelassen werden; kein pauschales Extension-Origin-Allowlisting.
8. Die Architektur nennt globale Aktionen, aber keine konkreten Chords. Gewählt wurden Super+Shift+R (Review), Super+Shift+Esc (Takeover), Super+Shift+Z (Öffnen), vollständig konfigurierbar. Redaktionsmuster sind zusätzliche, case-insensitive Teilstrings von Parameternamen. Redaktion entfernt URL-Zugangsdaten/Fragmente, sensible Queryparameter und nicht erlaubte DOM-Attribute; Freitext und Screenshot-Inhalte benötigen weiterhin die vorgesehene Sichtprüfung.

Die Tests prüfen Fake-HTTP-Flows einschließlich AT-10/11/14/15/18/19, WS-Authentifizierung/Sequenz, CLI, Datenbank-Rollback und Wiederöffnung, Zustandsautomaten, Shortcutprüfung und JSON-Vertrag. Echte Provider-/Browser-/Audioabnahme wird damit nicht behauptet.
