# Phase 0: Browser-Machbarkeit von Zuruf

Stand: 10.11.2026. Durchführung ausschließlich in `/home/mika/Projects/Humaify/zuruf/main`. Keine Commits, keine Änderungen an T3, Voxtype oder Benutzerkonfiguration. Die parallelen Spikes unter `spikes/t3`, `spikes/voxtype` und deren Bericht wurden nicht angefasst.

Die Architektur ist auf diesem Rechner grundsätzlich machbar. Nachgewiesen sind headed System-Chromium mit MV3-Extension, dieselbe CDP-Tab-ID für zwei Playwright-Clients, eine echte Playwright-MCP-Aktion, einfache Takeover-Sperre, lokale Panel-Einbettung, isolierte Cookies, Cookie-Persistenz und korrekte Ausschnitte bei Zoom/HiDPI. **Noch nicht nachgewiesen ist die harte deterministische Unterbrechung bereits laufender Agentenaktionen oder eine sichere Beschränkung des unveränderten MCP auf genau einen Tab.** Das ist vor Implementierung des Shared Mode als verbleibende P0-Arbeit einzuplanen.

## Umgebung und Reproduktion

Verifiziert durch lokale Befehle:

```text
node --version                  v26.12.0
pnpm --version                  11.18.0
chromium --version              Chromium 155.0.8059.84 Arch Linux
google-chrome-stable --version  Google Chrome 157.0.8103.38
hyprctl version                Hyprland 0.56.2
XDG_SESSION_TYPE               wayland
WAYLAND_DISPLAY                wayland-1
```

Omarchy/Arch, 14 GiB RAM, ungefähr 10 GiB belegt; Monitor physisch 1920×1200 bei Scale 1. Chromium-Fenster wurden tatsächlich geöffnet, `hyprctl clients -j` meldete `xwayland: false`. Kein Headless-Ersatz. Die im Spec erwähnte KDE-Plattform ist für diese Installation falsch. Installiert wurden nur lokale Spike-NPM-Abhängigkeiten: `playwright-core 1.65.0`, `pngjs 7.0.0`, `ws 8.22.2`, `@playwright/mcp 0.0.84` mit internem Playwright `1.65.0-alpha-2026-10-30`, TypeScript 7.0.2 und Typdefinitionen; Lockfile liegt bei.

[Startbefehle](../../spikes/browser/README.md), [ausführbarer Haupttest](../../spikes/browser/proof.ts) und [bereinigte vollständige Ausgabe](../../spikes/browser/evidence.json). Die erfolgreichen Hauptläufe endeten mit `status=0/SUCCESS`; Strict-TypeScript-Prüfung ebenfalls erfolgreich. Jeder Browser lief separat unter `systemd-run --user --wait --collect --pipe -p MemoryMax=3G -p MemorySwapMax=1G -- …`; der gesamte Testdienst zusätzlich unter derselben Grenze. Nie zwei eigene Chromium-Instanzen gleichzeitig. Alle HTTP-/WS-Listener nur `127.0.0.1`; temporäre Profile ausschließlich unter `.spike/browser/` (gitignoriert).

## 1. Systembrowser, MV3 und Debugging-Transport

**Verifiziert:** `/usr/bin/chromium --load-extension=<absolute Laufzeitkopie>` lädt die MV3-Extension ohne `DisableLoadExtensionCommandLineSwitch` und ohne `--enable-unsafe-extension-debugging`. Nachweise: Extension-Serviceworker-Target, authentifizierter WS-Handshake, reale Content-Script-Aktionen, Panel. Eine zusätzliche Chromium-Standardkomponente „Google Hangouts“ war im frischen Profil vorhanden; die gezielte eigene Extension-ID wird deshalb verwendet, nicht einfach das erste beliebige Extension-Target.

Die anfänglichen Fehlversuche mit „missing event hello“ waren kein Beweis einer entfernten CLI-Option: Beim Wiederverwenden eines Spike-Profils lief noch gecachter Serviceworker-Code mit altem zufälligem Port. Neue Profile pro Testlauf beseitigten das Problem. Deshalb die Dienstadresse im Produkt stabil konfigurieren oder per versionsgebundenem Bootstrap aktualisieren; nicht nur Dateien unter derselben Extension-Version überschreiben und sofortige Worker-Aktualisierung annehmen. Der Feature-Override wurde kurz versucht, ist hier aber nicht erforderlich und keine Empfehlung.

**Alternativ verifiziert:** [pipe.ts](../../spikes/browser/pipe.ts) startet System-Chromium durch Playwright über dessen Pipe und führt mit `--enable-unsafe-extension-debugging` erfolgreich `Extensions.loadUnpacked({path})` aus:

```json
{"pipeLoadUnpacked":{"id":"egjnpcmekhnieplhboioipdbpdakepbf"}}
```

Chrome-branded Builds haben `--load-extension` seit Chrome 137 entfernt; Chromium und Chrome for Testing sind laut [Chromium-Ankündigung](https://groups.google.com/a/chromium.org/g/chromium-extensions/c/1-g8EFx2BBY/m/my-groups) ausgenommen. Das lokal vorhandene Google Chrome wurde nur auf seine Version geprüft, nicht als Reviewbrowser verwendet. Die [CDP-Erweiterungsschnittstelle](https://chromium.googlesource.com/chromium/src.git/+/225b2eaa7f23c33b7c4e30c1bfc58f1bd99cbe1c%5E%21/) ist experimentell; Pipe plus Unsafe-Flag wurde real getestet, nicht als universelles Chrome-Versprechen behandelt.

**Empfehlung für diesen Rechner:** System-Chromium, überprüfte Extension-Handshake-Bedingung beim Start und zufälliger Loopback-CDP-Port. Damit kann öffentliches `connectOverCDP` ohne interne Playwright-Transporte attachen. Eine Pipe ist privater, aber kein Netzwerk-Endpunkt, den Playwright MCP mit `--cdp-endpoint` direkt übernehmen könnte. Dafür müsste zurufd selbst einen CDP-Pipe-Broker bereitstellen; dessen Multiplexing wurde hier nicht implementiert. Pipe-Laden ist ein nachgewiesener Ausweichweg, kein bereits fertiger attachbarer Broker.

Kein gebündelter Browser heruntergeladen. Lokales Chromium belegt 410 MiB, Spike-`node_modules` 67 MiB. Browser-cgroup-Spitzen im erfolgreichen Lauf ungefähr 180–246 MiB ohne Swap, nur für die kleine Fixture; das ist keine Obergrenze für echte Anwendungen. Ein zusätzlicher Browser dürfte größenordnungsmäßig mehrere hundert MiB entpackten Speicher und einen eigenen Updatepfad kosten (Schätzung, kein Downloadmaß). Bundling garantiert keinen niedrigen RAM-Verbrauch; nicht pauschal „zweite Engine im RAM“, wenn nur der gebündelte Browser gestartet wird.

## 2. Hotkeys und nicht scriptbare Seiten (AT-17)

Die Extension registriert `window`-Listener in Capture-Phase bei `document_start`, in allen passenden Frames (`all_frames`, `match_about_blank`). Der Guard untersucht den gesamten `event.composedPath()`: `input`, `textarea`, `select`, Contenteditable, ARIA-Text-/Comboboxen sowie Monaco/CodeMirror-Container und explizite Ignore-Marker. Er ignoriert Wiederholungen, IME-Komposition und Modifier. Modus und Mapping sind per Extension-Nachricht konfigurierbar.

**Real bestanden:** `wsd` wird normal in Input, Passwortfeld, Textarea, Contenteditable und offenem Shadow-DOM-Input geschrieben; keine Capture-Aktion. Auch der iframe-Input erhält `wsd`. Auf Select und Editorcontainer keine Capture-Aktion; bei ausgeschaltetem Review ebenfalls keine. Außerhalb löst `W` einen Capture aus, `S` aktiviert Drag-Auswahl und `D` den Picker. Die Fixture installiert einen eigenen `window`-Listener mit `stopImmediatePropagation`; der frühere Extension-Capture-Listener funktioniert trotzdem.

**Grenzen:** Dies beweist keinen universellen Vorrang gegen vor dem Content Script registrierte Listener. Geschlossene Shadow Roots verbergen den tatsächlichen Input im Eventpfad; dort braucht es konservatives Unterdrücken bei unbekannten fokussierten Custom Elements oder Modifier-Ausweichbelegung. Monaco wurde als Containerguard geprüft, kein komplettes Monaco-Paket geladen. IME, Bildschirmtastaturen, Layoutvarianten und reale komplexe Editoren bleiben manuell zu testen.

`all_frames` löst Fokus-/Cursor-Routing nicht automatisch: Hover im iframe, Tastaturfokus im Hauptframe, liefert im ersten Versuch noch die Hauptframe-Auswahl. Der erfolgreiche Frame-DOM-Test fokussiert den Frame vorher durch Klick. Produkt muss letzten Pointer-Frame pro Tab kennen und den Capture dorthin routen; Rechtecke in Top-Viewport-Koordinaten umrechnen. Cross-Origin-Frames benötigen passende Hostberechtigungen und Frame-Registry, keine unberechtigten Zugriffe vom Hauptframe aus.

`chrome://`, Chrome Web Store, fremde Extension-Seiten und der interne PDF-Viewer dürfen nicht als normale Content-Script-Ziele vorausgesetzt werden; der Spike matcht nur HTTP(S). Für diese Ziele sind Overlay/DOM-Picker nicht zugesagt. Eine äußere HTTP-PDF-URL macht den internen Viewer nicht zu normalem zugänglichem HTML. Diese Sonderseiten wurden nicht separat automatisiert getestet. File-URLs benötigen ausdrückliche Extensionfreigabe und sind im Spike ausgeschlossen. Hintergrund und Isolated World gemäß [Content-Script-Dokumentation](https://developer.chrome.com/docs/extensions/develop/concepts/content-scripts).

## 3. DOM-Capture und Redaktion (AT-07, teilweise AT-19)

Der Picker nimmt `elementFromPoint` und steigt in offene Shadow Roots hinab. Er liefert Kandidaten in der Reihenfolge Test-ID, Rolle+Name, aktuell eindeutiger Text, CSS, XPath; jeder mit `stable` und Begründung. Stabile Kennzeichnung bedeutet einen semantischen Vertrag, keine Garantie gegen spätere Änderungen. Text/CSS/Positions-XPath sind im Spike ausdrücklich instabil.

Metadaten enthalten Tag, erlaubte Attribute, begrenzten sichtbaren Text, Rolle/Name, Viewport-BBox, Scrollposition, Eltern-Tag, begrenzte Kind-Tagliste, URL ohne Query/Fragment, Shadow-Host-Pfad und bestmöglichen Iframe-Pfad. Der Serviceworker ergänzt echte Extension-`tabId` und `frameId`; diese sind von CDP-Target-/Frame-IDs zu unterscheiden. Same-Origin-Fixture: `iframePath` enthält `#frame` und Frame-URL; Shadow-Button enthält `shadowPath: ["#shadow"]`.

**Verifiziert:** Hauptbutton, Shadow-Button und Frame-Button werden korrekt identifiziert. Test-ID und Rolle+Name von Haupt-/Shadow-Button, Text und CSS werden via Playwright erneut aufgelöst; jeder ergibt genau einen Treffer. Der Frame-Test löst seine Test-ID über `frameLocator('#frame')` erneut auf. Positions-XPath funktioniert im normalen DOM; Shadow-XPath wird ausdrücklich nicht benutzt. Der tief unten liegende Button wird nach Scrollen mit passender Scrollposition und sichtbarer BBox erfasst.

Attribut-Allowlist statt vollständigem `outerHTML`: `value`, Passwort-/Token-/Secret-/Cookie-/Sessionattribute, URLs, Eventhandler und Styles werden nicht exportiert. Der Fixture-Button trägt absichtlich `data-token`; dieses fehlt im Capture. Passwort-Capture nach Blur enthält weder `value` noch den synthetischen Passworttext. Input-/Textarea-/Contenteditable-Inhalt wird vom Textfeldexport ausgeschlossen. Cookie-Werte werden nicht in den Nachweisbericht übernommen.

**Noch offen:** Namen im Content Script sind eine vereinfachte Heuristik (`aria-label`/Text), keine vollständige Accessible-Name-Berechnung; `aria-labelledby`, Labels, Hidden-/SVG-Sonderfälle und weitere implizite Rollen benötigen AX-Anreicherung oder geprüften AccName-Code. Cross-Origin-/mehrfach verschachtelte Frames, geschlossene Shadow Roots, doppelte Test-IDs, dynamische Listen und Navigation während Auswahl sind nicht vollständig bewiesen. Framepfade mit unauflösbarem Host werden als `unresolved` markiert, nicht geraten. Beliebiger sichtbarer Text, Klassen/IDs oder ARIA-Labels können trotzdem personenbezogene Daten enthalten; die Allowlist ersetzt keine inhaltliche Redaktion. Screenshots bilden sichtbare Daten weiterhin ab. AT-19 ist daher nur für die konkret geprüften Metadatenfälle erfüllt.

Zur Rekonstruktion nur Kandidaten mit genau einem Treffer und passendem Kontext akzeptieren; nach Navigation neue Dokument-Epoch prüfen. Bei null/mehreren Treffern historische Aufnahme anzeigen, keine stille Zuordnung.

## 4. Screenshot-/Region-Pipeline (AT-05/06)

**Verifiziert:** echte `W`-Keyevents → Content Script → Worker → localhost-WS → Node → `Page.captureScreenshot` → PNG auf Disk. Vor dem Screenshot wird der Overlay entfernt. Ein zweites PNG erhält rein serverseitig über `pngjs` einen Ring an der gemessenen Cursorposition; Rohdatei bleibt unverändert. Kein zweiter Seiten-Screenshot nötig, daher kein zusätzlicher Animations-/Layoutversatz durch einen temporär ins Dokument gezeichneten Ring.

`S` → Pointer-Down/Move/Up erzeugt ein Rechteck von 80×40 CSS-Pixeln innerhalb der einfarbigen Fixture-Fläche. Der Test vergleicht sowohl das tatsächlich gemeldete Drag-Rechteck als auch **jeden einzelnen Pixel** des CDP-Ausschnitts mit der erwarteten Farbe. Browserzoom wird über `chrome.tabs.setZoom` gesetzt, keine CSS-`zoom`-Simulation. DPR 1/2 wird über CDP emuliert, kein echter Scale-2-Monitor.

Für das hier verwendete Chromium ist `clip` in den Browser-DIP-Raum umzurechnen: `(viewportRect + cssVisualViewport.pageOffset) × browserZoom`, `scale:1`; die Ausgabe wird anschließend mit dem Basis-DPR gerastert. Ein unverändert übergebenes 80-CSS-Pixel-Rechteck erzeugte bei Zoom 125 % fälschlich nur 80 statt 100 Pixel. Die korrigierte Variante trifft Farbe und Maße bei allen vier Kombinationen. Für den Cursor verwendet der Compositor die gemessenen Verhältnisse `png.width / innerWidth` und `png.height / innerHeight`, keine feste DPR-Annahme. Bei anderen Browser-Versionen den Pixeltest erneut ausführen. Pinch-Zoom ist hier nicht geprüft.

| Basis-DPR | Browserzoom | Effektiver DPR | Region-PNG | W → Rohdatei | W → Ringdatei |
|---|---|---|---|---|---|
| 1 | 100 % | 1 | 80×40 | 62 ms | 112 ms |
| 1 | 125 % | 1.25 | 100×50 | 270 ms | 311 ms |
| 2 | 100 % | 2 | 160×80 | 163 ms | 289 ms |
| 2 | 125 % | 2.5 | 200×100 | 160 ms | 289 ms |

Das sind einzelne Laborwerte ab unmittelbar vor dem über Playwright ausgelösten Keypress bis nach `writeFile`, keine p95-/Dauerlastmessung und kein `fsync`-Nachweis. Die Node-WS-Pollschleife kann bis zu 50 ms beitragen. Animierte Seiten, Offscreen-Auswahl, Scrollen während Drag und fremde Frames benötigen weitere Abnahmetests. CDP zeigt Webseiteninhalt, keine Browserchrome und keinen nativen OS-Cursor.

## 5. Shared Control, Takeover und Playwright MCP (AT-02/03)

Zwei unabhängige `chromium.connectOverCDP`-Clients sehen denselben headed Haupttab. Der Dienst verbindet direkt mit dem rohen Browser-Endpunkt; der Agent über einen WebSocket-Proxy mit zufälligem Bearer-Token. `Target.getTargetInfo` liefert für beide exakt dieselbe ID (siehe `shared.targetId` im Nachweis). Agent klickt den Fixture-Zähler, Dienst sieht `data-count=1`.

Danach `owner='human'`: der Proxy weist **jeden neuen CDP-Befehl** des Agenten mit Fehler -32000 zurück; keine Warteschlange. Ein anschließendes `page.evaluate(...click())` schlägt fehl und der Zähler bleibt unverändert. `owner='agent:spike'` erlaubt den nächsten Klick und erhöht auf 2. Der Proxy prüft zusätzlich die Lease-Zeit vor jedem Befehl. Der Fristablauf selbst wurde nicht separat provoziert. Unauthentifizierte Verbindungen zum Agentpfad sind im Code ausgeschlossen, nicht als eigener Negativtest ausgeführt.

**Wichtige Überraschung:** Playwright zeigt beim abgefangenen Evaluate nicht den Proxyfehler an, sondern:

```text
Error: page.evaluate: Execution context was destroyed, most likely because of a navigation.
```

Im Labor war das kein tatsächlicher Navigationswechsel. Der Proxy zählt die zurückgewiesenen Befehle, und der unveränderte Zähler belegt, dass die Mutation nicht ausgeführt wurde. Die Anwendung braucht daher einen eigenen sichtbaren Takeover-Status und soll sich nicht auf Fehlermeldungstexte von Playwright verlassen.

Zusätzlich wurde `@playwright/mcp` als echter stdio-Prozess initialisiert und mit `--cdp-endpoint ws://127.0.0.1:<port>/agent --cdp-header 'Authorization: Bearer <ephemer>'` verbunden. `tools/list` und `browser_evaluate` funktionierten; der MCP-Aufruf erhöhte denselben Zähler auf 3. Dafür wurde kein zusätzlicher Browser gestartet. Die [MCP-Verbindungsdokumentation](https://playwright.dev/mcp/configuration/browser-extension) beschreibt CDP/Extension-Modus, die [Profil-Dokumentation](https://playwright.dev/mcp/configuration/user-profile) persistente/isolierte Profile. Die lokale `--help`-Ausgabe bestätigt `--cdp-endpoint`, `--cdp-header`, `--extension`, `--profile-dir-name`, `--user-data-dir`, `--isolated` und `--shared-browser-context`. Extension-Modus benötigt eine weitere Playwright-Extension und wurde nicht installiert oder getestet.

**P0-Grenzen, nicht als gelöst melden:**

- Das unveränderte MCP hat in der geprüften CLI keinen Parameter zur harten Bindung an eine einzelne bestehende Tab-ID. Die Toolliste enthält `browser_tabs` und `browser_run_code_unsafe`. Profilwahl ist keine Tab-Autorisierung. Ein Endpoint pro Session schützt vor versehentlichem Wechsel in andere Browserprozesse, nicht vor anderen Targets derselben Session.
- Der Laborproxy leitet nach Authentifizierung grundsätzlich alle Browser-Targets weiter. Panel-, Extension-, neue Tabs und Browserverwaltung sind noch nicht gefiltert. Er ist kein fertiger sicherer CDP-Broker. Auch `Browser.close` müsste für Agenten grundsätzlich verboten werden.
- Bereits vor dem Takeover weitergeleitete Befehle können weiterlaufen. Ein bereits gestartetes Promise, Script oder eine Navigation kann nach dem Takeover noch Effekte haben. Ein Proxy kann diese nicht rückwirkend zurücknehmen. `Runtime.terminateExecution` oder `Page.stopLoading` wären zusätzliche, möglicherweise störende Eingriffe, keine allgemeine Garantie.
- Mehrere Agenten, gleichzeitige neue Verbindungen, ausstehende Kommandos, Maus-down beim Takeover und Browserabstürze wurden nicht gestresst. Die globale Variable im Spike beweist die einfache Sperre, keine vollständige linearisierbare Mehrbenutzersteuerung.
- Der rohe Chromium-Port ist loopbackgebunden, aber ohne CDP-Authentifizierung. Wer als lokaler Prozess den Port kennt, könnte den Proxy umgehen. Prozess-/OS-Trustgrenze oder ein privater Pipe-Broker sind nötig, wenn untrusted lokale Agenten tatsächlich eingesperrt werden sollen.

**Empfehlung:** Autorisierung/Serialisierung zusätzlich an der MCP-Toolgrenze: Session+Tab+Dokument-Epoch+Agent+Lease zu jedem Auftrag, eine Ausführungsqueue je Tab, Takeover sperrt sofort neue Aufträge, widerruft die Lease und markiert laufende Aufträge als „Abbruch angefordert / Ausgang unklar“. Erst nach bestätigtem Stillstand Human-Control als exklusiv bestätigen. Proxy als zweite Sperre, nicht als Beweis totaler Deterministik. Kein wiederholtes Queue-Abspielen alter Klicks nach Freigabe. P0 muss die Semantik laufender Aktionen konkret beschließen und mit Race-Tests belegen.

## 6. Cookie-Isolation und Persistenz (AT-01/04)

Die Fixture `/login` setzt serverseitig ein ausschließlich synthetisches `HttpOnly`-freies Testcookie mit `Path=/; Max-Age=3600; SameSite=Lax`. Der Browser meldet es in der Standardsitzung. Ein neu erzeugter Playwright-Kontext enthält keine Cookies; danach wird dieser geschlossen. Der Dienst beendet Chromium kontrolliert über `Browser.close`, wartet auf Prozessende und startet **denselben Profilpfad** erneut. Das Cookie bleibt vorhanden. Ein anschließend allein gestartetes zweites Profil enthält wiederum keine Cookies. Alle drei Behauptungen sind Assertions im erfolgreichen Lauf.

Frühe Versuche mit sofortigem Beenden und Schließen über den Agentclient verloren das Cookie. Der erfolgreiche Ablauf nutzt einen serverseitigen Cookie-Set, 1,2 s Ruhe vor dem kontrollierten Shutdown und nur den verwaltenden CDP-Client zum Beenden. Welche frühere Änderung allein ursächlich war, wurde nicht isoliert; daraus keine garantierte sichere Crash-Persistenz ableiten. Browser nicht nach letztem Login sofort hart terminieren.

Das ist ein Cookie-Persistenzbeweis, **kein menschlicher SSO-/2FA-Login**. Auslaufende Sessions, Session-only-Cookies, Website-Abmeldung, verschlüsselte Authdaten und Betriebssystem-Keyring bleiben website-/browserabhängig. `--password-store=basic` ist ausschließlich Laborbequemlichkeit und kein empfohlener Passwortspeicher für Zuruf. Profilzustände niemals ins Manifest oder in Agentprompts exportieren.

## 7. Side Panel, localhost-UI und App-Fenster

**Im normalen Reviewfenster verifiziert:** `sidePanel.open()` ohne Benutzeraktion lehnt mit „may only be called in response to a user gesture“ ab. `P` im Content Script → `runtime.sendMessage` → unmittelbarer Worker-Aufruf öffnet es erfolgreich. CDP meldet ein `panel.html`-Page-Target plus einen geladenen `http://127.0.0.1:<port>/ui`-Iframe. Die Panel-Seite wird dabei nicht als normale Playwright-`context.pages()`-Page angeboten; der Nachweis nutzt CDP-Targets. Es wurde keine menschliche Sichtprüfung der Panelpixel gemacht.

Das Panel muss eine Extension-Datei als `default_path` verwenden. Ein direktes localhost-URL als Panelpfad ist nicht die unterstützte Form. Die Extension-Datei kann den lokalen UI-Server über ein Iframe einbetten; das funktioniert mit explizitem `frame-src http://127.0.0.1:*`. Der Fixture-Server verbietet Embedding nicht. Produktserver muss passenden `frame-ancestors chrome-extension://<feste-id>` erlauben; kein widersprechendes `X-Frame-Options: DENY/SAMEORIGIN`. Remote React-Bundle direkt als Script in die MV3-Extension laden ist keine robuste Alternative: entweder gebündeltes UI innerhalb der Extension oder localhost-App im Iframe. CSP und UI-Herkunft gemäß [Side-Panel-API](https://developer.chrome.com/docs/extensions/reference/api/sidePanel).

Der Nachweis verwendet nur einen lokalen HTML-Platzhalter. React ist hier weder gebaut noch notwendig, um Transport/Embedding zu prüfen. Empfohlen sind zwei Build-Ausgaben aus gemeinsamen React-Komponenten: Extension-Bundle für das Panel und Web-Bundle für Companion; beide sprechen mit zurufd. Iframe ist für die lokale Entwicklung eine funktionierende Option. Kein Token in iframe-URL/Seiten-DOM; explizite, herkunftsgeprüfte Bootstrap-Kommunikation.

**`--app=` separat verifiziert:** Es entsteht ein echtes Companion-App-Fenster mit `/ui`. Beim Versuch, darin per `P` das Side Panel zu öffnen, meldet die API zwar `ok:true`, aber es entstehen **keine Panel-/Iframe-Targets**. Deshalb ist „App-Fenster plus Side Panel“ kein tragfähiger Default. Empfehlung: normaler Chromium für Reviews und Side Panel, eigener `--app=`-Companion nur für Board/Settings. Nach Möglichkeit eigener nicht agentfreigegebener UI-Profilbereich; derselbe Browserprozess mit ungefiltertem CDP würde auch Companion/Panel dem Agenten ausliefern. Physische Fenster-/Fokuswirkung auf dem Desktop muss der Nutzer noch abnehmen.

Ein Side Panel kann standardmäßig **verfügbar** gemacht werden, nicht voraussetzungslos bei Dienststart **geöffnet** werden. Toolbar-Action, erlaubter Shortcut oder ein bewusster Review-Start-Klick sind geeignete Einstiegspunkte.

## 8. Hyprland statt KDE

`--ozone-platform=wayland` funktioniert; `xwayland:false` ist geprüft. Weitere historische Ozone-Featureflags waren nicht nötig. Ob Chromium auf dieser Installation ohne das Flag zuverlässig automatisch Wayland wählt, wurde nicht geprüft; daher explizit setzen.

Auf Hyprland 0.56.2 mit Lua-Konfiguration sind alte Beispiele falsch:

```text
hyprctl keyword bind 'SUPER SHIFT CTRL ALT,F24,exec,/usr/bin/true'
keyword can't work with non-legacy parsers. Use eval.
```

[hyprland.ts](../../spikes/browser/hyprland.ts) hat stattdessen real registriert, per `hyprctl binds -j` gefunden und im `finally` wieder entfernt:

```bash
hyprctl eval 'hl.bind("SUPER + SHIFT + CTRL + ALT + F24", hl.dsp.exec_cmd("/usr/bin/true"), {description="Zuruf Phase-0 temporary proof"})'
hyprctl eval 'hl.unbind("SUPER + SHIFT + CTRL + ALT + F24")'
```

Ausgabe jeweils `ok`; Nachprüfung `remaining:0`. Die Kombination wurde nicht physisch gedrückt. Im Produkt ruft der Dispatcher einen festen absoluten `zuruf`-CLI-Pfad auf, der authentifiziert mit zurufd spricht. Kein Webseiteninhalt wird in Shell/Lua-Ausdrücke interpoliert. Konfigurationsreload entfernt Laufzeitregistrierungen; Reconnect-/Reload-Ereignisse müssen eine konfliktgeprüfte Neuregistrierung auslösen. Kein permanenter Source-Eintrag nötig. Eine später ausdrücklich installierte app-eigene Lua-Datei mit einem `require` wäre eine persistente Alternative; hier wurde nichts unter `~/.config/hypr` verändert.

Auch alte `hyprctl dispatch setfloating …`/`resizewindowpixel …` scheiterten mit Parserfehler. Gegen ausschließlich das eigene Fixture-Fenster funktionierten die folgenden **Lua-Dispatcher** über `hyprctl dispatch` jeweils mit `ok`:

```lua
hl.dsp.window.float({action="set",window="address:<eigene-adresse>"})
hl.dsp.window.resize({x=900,y=700,relative=false,window="address:<eigene-adresse>"})
hl.dsp.window.move({x=50,y=60,relative=false,window="address:<eigene-adresse>"})
```

Die native Wayland-Fenstersteuerung des Compositors ist damit möglich. Die Befehlsannahme ist geprüft, finale Bildschirmgeometrie/Animation wurde nicht nachgemessen. Nur Floating-Fenster frei positionieren; Tiling-/Fullscreen-/Monitorregeln beachten. Quellen: [aktuelle Binds](https://wiki.hypr.land/configuring/core/binds/), [hyprctl](https://wiki.hypr.land/configuring/core/advanced-configuration/using-hyprctl/), [Dispatcher](https://github.com/hyprwm/hyprland-wiki/blob/main/content/configuring/core/dispatchers.md). Version erkennen; Legacy- und Lua-Syntax nicht mischen.

Globale W/S/D-Binds wären auch bei Texteingaben außerhalb des Browsers aktiv und dürfen den Content-Script-Guard nicht ersetzen. Global nur bewusste Modifier-Shortcuts für Review/Takeover/Dispatch. Kein generelles Wayland-Input-Sniffing, kein zuverlässiges Ermitteln des DOM-Fokus beliebiger fremder Anwendungen und kein universelles Positionieren über normale Web-APIs. Auch Hotkeys im Lockscreen sind nicht automatisch Teil der Berechtigung.

## Konkrete Zielarchitektur und Protokoll

1. **zurufd:** ein Node-26-Dienst mit Loopback-HTTP/WS, SQLite/Assetablage, Sessionregistry, Capturequeue und Control-Leases. Verwaltet Start/Exit jeder eigenen Chromium-Unit und deren Speicherlimit; Produktionsbudget für mehrere Sessions separat festlegen. Auf diesem Rechner parallele aktive Browser sparsam begrenzen, inaktive Profile auf Disk halten.
2. **Sessionlayout:** `~/.local/share/zuruf/sessions/<uuid>/profile/` und separat `reviews/<review>/batches/<batch>/assets/`. Profile chmod 0700, keine Symlinks auf Nutzerprofile, keine unbeabsichtigten Cookieklone. Session-ID, Profile-ID, Tab-ID und Review-ID getrennt. Ein Prozess/Profil je dauerhaft isolierter Session als einfachster Default; temporäre Kontexte für flüchtige isolierte Tests.
3. **Extension:** statische Content Scripts in isolierter World, frühe Capture-Listener, Fokusguard, Pointer-Frame-Registry, Picker/Drag. Worker hält Chrome-API-Kommunikation und WS; Panel ist eigene Extensionseite. Kein MAIN-World-Binding, das Webseiten beliebige Daemonbefehle erlaubt. Bevorzugt Worker↔WS gegenüber CDP-Bindings, weil Extension-APIs und Frameidentität damit unabhängig von Agent-CDP-Verbindungen bleiben. Der Spike beweist diesen Transport; Heartbeat/Resume/Quittierung sind noch Produktarbeit. MV3-Worker können schlafen: Zustand in `chrome.storage.session`, authentifiziert reconnecten, verlorene Nachrichten persistent wiederholen. Aktive WebSocket-Nachrichten können den Idle-Timer zurücksetzen, siehe [Worker-Lebenszyklus](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle).
4. **Nachrichtenvertrag (Vorschlag):** `v`, `requestId`, `sessionId`, `tabId`, `frameId`, `documentId/epoch`, `reviewId`, `batchId`, `type`, `timestamp`, `payload`; Server ergänzt vertrauenswürdige Zuordnung aus dem authentifizierten Kanal. Typen `hello/resume`, `configure`, `pointer`, `capture.request`, `overlay.hide`, `overlay.hidden`, `capture.saved`, `capture.failed`, `control.changed`, `ack`. Dedup nach Request-ID; keine frei wählbaren Dateipfade oder auszuführenden Scripts in Webseitenmessages. Worker trägt Auth nur aus eigener privater Bootstrapkonfiguration, Origin und Sitzungsbindung serverseitig prüfen. Der Spike authentifiziert WS mit ephemerem Token plus Extension-Origin und Agent-CDP mit Bearer-Header; vollständige Schemaprüfung/Ratenlimits fehlen noch.
5. **Capture:** Request an aktives Batch binden → Tab/Frame/Epoch validieren → Overlay-Hide in allen beteiligten Frames quittieren lassen → Layout-/Zoom-/DPR-Metadaten lesen → CDP-Shot/Clip → Raw-PNG atomar schreiben → pngjs-Ringkopie → Metadaten/Assets transaktional verknüpfen → Overlay wieder freigeben. Im Spike normaler `writeFile`, noch keine atomaren Crash-Transaktionen. Für Framelokatoren Parentpfade aus Browser-/Extensionregistry ergänzen. Accessible Name aus AX validieren, Inhaltsredaktion vor Persistenz.
6. **Agenten:** je Session explizite MCP-Instanz bzw. gekapselte Verbindung; CDP-Header-Token, Tool-Gate und targetbewusster Broker. Browser-/Sessionrechte standardmäßig entzogen. Keine direkte Weitergabe des rohen Browserports an Agenten. Kein Anspruch auf vollständige Session-Sicherheit allein durch einen zufälligen Port oder MCP-Profilnamen. Vor Freigabe müssen Ziel-Filter, neue Targets, fremde Kontexte, Paneltargets, `Browser.close`, laufende Evaluate-Aufträge und Takeover-Rennen getestet werden.
7. **Desktop:** zustandsarme `zuruf`-CLI → zurufd. Hyprlandadapter erkennt Lua/Legacy, registriert nur freie explizit konfigurierte Modifier-Shortcuts, entfernt exakt eigene Bindings beim Exit und stellt sie nach Reload wieder her. Fenster über PID plus bekannte Sessionregistrierung identifizieren, nicht über beliebige Seitentitel; der Titelvergleich des Spikes ist nur für dessen bekannte Fixture gedacht.

## Offene Abnahme / Blocker

Keine fehlende Wayland-Verbindung und kein Browserdownload-Blocker. Die grundlegenden Browserfunktionen funktionieren headed. Vor MVP bleiben verpflichtend: harte Takeover-Semantik laufender Aktionen, tabgenaue MCP/CDP-Autorisierung, Pointer-/Focus-Routing über Frames, vollständige Accessible Names und umfassendere Datenredaktion. Der Spike enthält weder fertige React-UI noch fertigen Daemon/MCP-Broker.

Manuell am echten Desktop: realer menschlicher Login einschließlich 2FA mit Restart; physische W/S/D und Modifier in verschiedenen Layouts/IME sowie realen Editoren; menschliches Takeover während laufendem Agentenscript/Drag/Navigation; Fokuswechsel zwischen Browser, Panel und Companion; sichtbare Paneldarstellung und Fenstergeometrie; echte Monitor-DPR-2-/Mischskalierung, Browserzoom und Scroll-/Frameauswahl; globale Taste außerhalb des Browsers nach Runtime-Registrierung und nach Hyprlandreload. Separat technisch: Cross-Origin-/nested Frames, geschlossene Shadow Roots, geschützte Seiten/PDF, Worker-Idle/Reconnect, Crash/Recovery und dynamische Layouts. Die Befunde sind eine Phase-0-Funktionsprobe, keine vollständige AT-01–20-Abnahme.
