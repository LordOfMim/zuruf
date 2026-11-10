# zuruf-browser

Rust-Browserbaustein für den Daemon, ohne Abhängigkeit auf `zuruf-core`. Kein eigener HTTP-Server in der Bibliothek. Der Daemon bindet ausschließlich Loopback, prüft Upgrade-Pfad und Herkunft und übergibt akzeptierte `tokio_tungstenite::WebSocketStream`s. Das Labor besitzt einen kleinen, ausdrücklich nicht produktiven Fixture-HTTP-Server.

## Öffentliche Integration

| Vertrag / zukünftiger BrowserController | Implementierung |
|---|---|
| Session starten | `session::Browser::start(SessionConfig, ExtensionHub)` |
| Stoppen, Neustart mit URLs | `Browser::stop`, `Browser::restart` |
| Isoliert duplizieren | `Browser::duplicate_config(new_root)`; Daemon erzeugt dafür eine neue `Bootstrap`/Hub/Session und startet die zurückgegebene Config. Kein Profil-/Cookieklon. |
| Tab öffnen / Liste | `Browser::open_tab`, `Browser::list_tabs` |
| Capture | `Browser::capture_request(envelope, Ring)` (Epoch-Prüfung); direkt `capture(target_id, CaptureKind, Option<Rect>, Ring)` → `CaptureResult` |
| Reviewmodus / Bindings | `Browser::set_review_mode(on, bindings)`, `Browser::push_bindings`; alternativ getrennt über `ExtensionHub::set_review_mode` / `push_bindings` |
| Takeover / Release / Grant | `Browser::takeover`, `release`, `grant(agent_id, Duration)` |
| Events | `Browser::events()` → Tokio-Broadcast; `BrowserEvent::{TabsChanged,Crashed,ControlChanged,Extension}` |
| `/ext` | Hub **vor Browserstart** anlegen: `ExtensionHub::new(Browser::bootstrap(port, session_id))`; `hub.serve(ws, upgrade_origin, trusted_extension_id)` |
| `/cdp/<sessionId>` | Session aus dem Pfad serverseitig auflösen; `Browser::serve_agent(ws, AgentAccess, authorization_header)` |
| Agententoken widerrufen | `AgentAccess::new` einmal pro Berechtigung erzeugen, Klone an Verbindungen geben; `revoke()` sperrt auch vorhandene Klone. Daemon darf widerrufene Objekte nicht neu erzeugen/weiterreichen. |
| Labor-Metriken | `set_zoom` nutzt echte `chrome.tabs.setZoom`, `emulate_dpr` CDP-Emulation. Nicht für normale Agentenrechte nötig. |

`types.rs` spiegelt `DomCapture`, `LocatorCandidate`, `Rect`, `Binding`, `Chord`, `ControlState`, `BrowserTab` mit camelCase-Serde. Die ID des `BrowserTab` ist momentan die CDP-Target-ID, keine UUID; der Daemon muss eine persistente UUID-Zuordnung ergänzen, falls er Tabs dauerhaft referenziert. Extension-`tabId`/`frameId` bleiben separate numerische IDs; `chrome.debugger.getTargets()` liefert die überprüfbare Zuordnung. Die zusätzliche `debugger`-Berechtigung dient ausschließlich dieser Abfrage, nicht dem Attach. Wire-Typen werden nicht automatisch aus TypeScript generiert. Der Daemon sollte diese Spiegeltypen an seiner Grenze in Core-Typen konvertieren und Action-/Binding-Werte zentral validieren. `leaseExpiresAt` ist momentan `None`; die Bibliothek prüft die echte Lease monoton, der Daemon ergänzt den darzustellenden RFC3339-Zeitpunkt.

`CaptureResult` enthält rohe und markierte PNG-Bytes sowie URL ohne Query/Fragment, Titel, Viewport/DPR, Scroll, Cursor, optionales Rechteck und DOM, Tab. Nur das Labor schreibt Dateien. Produktpersistenz, Batchbindung, atomare Assetwrites, `fsync` und Annotationstransaktionen gehören dem Daemon. Beide Bilder sind fertig, bevor das Result zurückkehrt; frühes Persistieren des Rohbilds vor der Komposition benötigt noch eine zweistufige Capture-API.

## Prozesse und Authentifizierung

Chromium startet headed auf Wayland, mit privatem Profil, zufälligem Loopback-CDP-Port und eigener systemd-Scope (`MemoryMax=3G`, `MemorySwapMax=1G`). `DevToolsActivePort` wird vor Start entfernt und danach gelesen. Jede Laufzeitkopie der Extension hat einen neuen Pfad und eine private `bootstrap.json`. Der öffentliche Manifest-Key fixiert die Extension-ID; er ist kein Secret. Produktionspfade müssen vom Daemon vergeben werden und dürfen niemals auf Benutzerprofile zeigen.

Die Extension sendet zuerst `{type:"auth",token}`; der Server prüft Token und die **exakte** erwartete Extension-Origin. Agenten authentifizieren sich über den Bearer-Header. `proxy::authenticate` steht für eine Prüfung schon vor dem HTTP-Upgrade zur Verfügung; `serve_agent` prüft erneut. Das Labor authentifiziert erst nach Upgrade. Produktionsrouting darf den privaten CDP-Endpunkt nicht ausgeben.

`Browser.close` sendet nur der verwaltende Client. Anschließend wartet der Manager auf den Prozess; nach fünf Sekunden stoppt er genau seine Scope. Auch beim Drop wird deren Stop angefordert. Ein WebSocket-Abbruch meldet `Crashed`, außer bei bewusstem Stop. Neustart verwendet die zuletzt gemeldeten URLs im Speicher. Crashfeste URL-Persistenz bleibt Daemonaufgabe. Cookiepersistenz ist mit synthetischem langlebigem Servercookie nach 1,2 Sekunden Ruhe und geordnetem Neustart geprüft. Kein Versprechen für sofortigen Crash, Sessioncookies, SSO oder menschliche 2FA.

## Kontrollgrenze

Alle neuen Proxybefehle prüfen Agent und Lease. FIFO je Tab verhindert vertauschte Maus-down/up-Befehle; eine gemeinsame Arbitration serialisiert auch mehrere Proxyverbindungen. Wartende Befehle tragen ihre Grant-Generation und werden nach Takeover/erneutem Grant niemals nachgeholt. Takeover widerruft sofort, meldet `settling`, wartet 500 ms und sendet über den Manager `Page.stopLoading` an Seiten. Danach endet `settling`.

**Bereits ausgeführtes JavaScript wird dadurch nicht beendet.** Der Race-Test startet einen 900-ms-Timer, übernimmt nach 100 ms und beobachtet dessen späte DOM-Änderung trotz Sperre neuer Befehle. Keine Verwendung von `Runtime.terminateExecution`, kein Zurückrollen bereits ausgeführter Aktionen. Eingabesequenzen mehrerer separat angemeldeter Agenten sind nur befehlsweise serialisiert; MCP-Auftragsgrenzen muss der Daemon zusätzlich sichern.

Browserweite CDP-Methoden sind standardmäßig gesperrt. Discovery/Autoattach zeigen nur HTTP(S)- und about:blank-Page-Targets; Worker/Panel/Extension bleiben verborgen. `Target.attachToBrowserTarget` wird durch eine virtuelle, weiterhin gefilterte Browsersession ersetzt. Fremde Targets, Browserkontexte und `Browser.close` sind gesperrt. `Browser.setDownloadBehavior` wird als autorisierter No-op quittiert, weil stock Playwright es beim Connect benötigt; Agentdownloads und Fensteränderungen sind dadurch nicht unterstützt. Autoattach pausiert neue Targets nicht (`waitForDebuggerOnStart:false`), damit ausgefilterte Targets nicht hängen bleiben. Browserrechte, Dateisystem-/IO-Domains und Kontextverwaltung sind nicht vollständig Playwright-kompatibel.

Der rohe Chromium-Port ist nur Loopback, aber Chromium authentifiziert ihn nicht. Ein lokaler Prozess, der ihn ermittelt, kann den Proxy umgehen. Das ist keine OS-Sandbox für bösartige lokale Agenten; eine private Pipe bleibt spätere Härtung.

## Extension und Capture-Grenzen

- Reviewmodus startet aus. Bindings verwenden `KeyboardEvent.code` und exakte Modifier. Untrusted DOM-Keyevents, Repeats und Komposition werden ignoriert. Guard für Eingaben, Contenteditable, ARIA-Textbox/Combobox, Monaco/CodeMirror, Ignore-Marker und fokussierte unbekannte Custom Elements ohne offenen Shadow Root.
- Ein geschlossener Overlay-Shadow-Root am `documentElement`; nur `static/overlay.css` gestaltet die semantischen Klassen und CSS-Variablen. Screenshots erfordern bestätigtes Hide aller bekannten lebenden Frames. Keine Styles/Attribute auf Zielseitenelementen.
- Offene Shadow Roots und einfache Same-Origin-Frames sind geprüft. AX-Rolle/Name stammen aus `Accessibility.getPartialAXTree`. Nicht auflösbare Framepfade werden ausdrücklich markiert; deren Rolle/Name werden verworfen. Cross-Origin-/OOPIF-, CSS-transformierte und mehrfach geschachtelte Frames sowie Pinch-Zoom sind nicht vollständig unterstützt/geprüft. XPath wird im Shadow DOM ausgelassen; bei IDs wird derzeit CSS statt zusätzlichem XPath geliefert.
- Attribut-Allowlist und Unterdrückung von Eingabewerten; URLs verlieren Query/Fragment. Beliebiger sichtbarer Text/ARIA-Label und Bilder können weiterhin personenbezogene Daten enthalten. Eine konfigurierbare Inhalts-/Bildredaktion ist noch nicht implementiert. Der Worker prüft `documentId` vor Weitergabe, `capture_request` prüft die Frame-Epoch vor/nach Capture und die Pipeline vergleicht den CDP-Framebaum vor/nach Screenshot. Same-Document-Layoutänderungen sind kein Navigationsevent und können weiterhin zwischen DOM-Messung und Bild passieren.
- Scriptbarkeit setzt einen tatsächlichen Content-Script-Handshake voraus; unzugängliche Seiten werden nicht als erreichbar geraten. Chrome-/Store-/PDF-Sonderseiten wurden noch nicht real automatisiert abgenommen.
- Heartbeat, reconnect, Resume und requestId-Dedup sind implementiert; zuverlässige Nachrichten liegen in `chrome.storage.session`. Der Puffer ist auf 1024 begrenzt und meldet Überlauf sichtbar. Dedup-Fenster: 4096 Nachrichten, nicht dauerhaft über Daemonneustarts. Pointer/Tabzustand wird bei Reconnect aktualisiert; ein echter erzwungener MV3-Idle-/Resume-Lauf fehlt noch.
- `panel.toggle` öffnet/schließt mit den Side-Panel-APIs ab Chromium 142. Toolbar und erster Review-Hotkey öffnen mit Benutzergeste. UI-Bundle wird als `panel/entry.js` erwartet; bis dahin deutscher Platzhalter. `comment.text`, Sprache und Dispatch gehen als Aktionen an den Daemon; Textdialog/Transkription sind nicht Teil dieser Extension. Aufnahmepill erhält `Message::Status { recording }` vom Daemon.

## Labor und Tests

Alle Befehle **in diesem Worktree** ausführen:

```bash
cd /home/mika/Projects/Humaify/zuruf/worktrees/browser
pnpm install --frozen-lockfile
pnpm --dir apps/extension check
pnpm --dir apps/extension lint
pnpm --dir apps/extension build
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo run -p zuruf-browser --example lab -j 2
```

W/S/D schreiben `.spike/browser-lab/*-{raw,marked}.png` und JSON-Metadaten; E/R/Shift+N sind Daemon-Aktionshooks. Strg+C schließt den Browser. Das Labor ist kein Daemonersatz. Sein restart-Profiltausch ist nur für die automatisierte Prüfung gedacht; der Capture-Listener löst die aktuelle Instanz pro Ereignis neu auf.

```bash
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo test -p zuruf-browser -j 2
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo clippy -p zuruf-browser --all-targets -j 2 -- -D warnings
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo fmt --all --check
# Prebuilt lab is required by the ignored integration-test binary:
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo build -p zuruf-browser --example lab -j 2
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo test -p zuruf-browser --test headed -j 2 -- --ignored --nocapture
# Performance run with the optimized binary:
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo build -p zuruf-browser --example lab --release -j 2
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- target/release/examples/lab --check
```

Der Test lädt keine Browser herunter, öffnet nur eine Chromium-Instanz gleichzeitig, startet stock MCP über stdio und verwendet ausschließlich synthetische Fixtures. Ergebnisse: `.spike/browser-lab/evidence.json`. Details und Messwerte im [Abnahmebericht](../../tests/browser/README.md).

Manuell offen: echter Login/2FA mit Neustart; physische Tastaturen/IME und reale Editoren; sichtbares Panel/Fokus/Overlay-Styling; Takeover während Drag/Maus-down und Navigation; reale DPR-2-/Mischskalierungsmonitore; unzugängliche Sonderseiten; erzwungener Worker-Schlaf, Crash-/Recovery-Stresstest. Keine vollständige AT-01–19-Produktabnahme.
