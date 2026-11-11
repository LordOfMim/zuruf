# zuruf-browser

Browser-Sitzungen für den Daemon, ohne Abhängigkeit auf `zuruf-core` und ohne eigenen HTTP-Server. Eine Sitzung ist ein Prozess von [`zuruf-shell`](../zuruf-shell/README.md) mit eigenem Profil; diese Bibliothek startet ihn, spricht über CDP mit seinen Seiten und über stdin mit seinem Fenster.

## Öffentliche Schnittstelle

| Aufgabe | Aufruf |
|---|---|
| Sitzung starten, stoppen, neu starten | `Browser::start(SessionConfig)`, `stop`, `restart` (mit den zuletzt offenen Adressen) |
| Isoliert duplizieren | `Browser::duplicate_config(new_root)` – neues Profil, keine Cookies |
| Tabs | `open_tab(url)` öffnet im Fenster der Sitzung, `list_tabs()` |
| Fenster | `focus()`, `show_panel(Some(true) \| Some(false) \| None)`, `set_theme("system" \| "dark" \| "light")`, `set_page_width(Some(390) \| None)` |
| Aufnahme | `capture_request(&Envelope, Ring)` für einen Hotkey (prüft die Dokumentkennung), `capture(target, kind, rect, ring)` direkt; `capture_raw`-Varianten liefern nur das maskierte Rohbild |
| Review-Modus, Belegung | `set_review_mode(on, bindings)`, `push_bindings`, `set_recording(bool)` |
| Kontrolle | `grant(agent, lease)`, `renew`, `release`, `takeover` |
| Ereignisse | `events()` → `BrowserEvent::{TabsChanged, Crashed, Closed, ControlChanged, Page}` |
| Agenten | `serve_agent(ws, AgentAccess, authorization)` hinter `/cdp/<sessionId>`; `proxy::authenticate` vor dem Upgrade |
| App-Fenster ohne Sitzung | `AppWindow::start(shell, directory, url, theme, args)`, `focus`, `set_theme`, `alive`, `stop` |

`types.rs` spiegelt `DomCapture`, `LocatorCandidate`, `Rect`, `Binding`, `Chord`, `ControlState`, `BrowserTab` mit camelCase-Serde. Die ID eines `BrowserTab` ist die CDP-Target-ID; der Daemon ordnet ihr eine dauerhafte UUID zu. Persistenz, Batchbindung und Dateien gehören dem Daemon.

## Seiten-Brücke

`bridge.rs` ersetzt die frühere Extension. Für jede Seite der Sitzung – und jeden Frame in einem anderen Prozess – gilt:

1. `Page.enable`, `Runtime.enable`. Ohne die Page-Domain registriert Chromium das Skript zwar, führt es in später geladenen Dokumenten aber nicht aus.
2. `Runtime.addBinding` mit `executionContextName: "zuruf"`: Die Funktion `__zurufSend` existiert nur in der isolierten Welt des Agenten.
3. `Page.addScriptToEvaluateOnNewDocument` mit `worldName: "zuruf"` und `runImmediately`: Der Agent (`apps/page`, zur Bauzeit eingebettet) läuft in jedem Frame, vor den Skripten der Seite.

Der Agent meldet `ready`, `visibility`, `title`, `pointer`, `action` und `capture.request`. Die Brücke ordnet jede Meldung über ihren Ausführungskontext einem Tab, einem Frame und einem Dokument zu. Befehle (`configure`, `status`, `control.changed`, `overlay.hide`/`show`, `capture.saved`/`failed`, `dom.request`) wertet sie in derselben Welt aus. Ein neu geladenes Dokument erhält auf `ready` sofort Belegung, Aufnahmestatus und Kontrollbesitzer.

Was Chromium selbst nicht meldet, liefert der Agent: Titeländerungen eines Tabs und welcher Tab sichtbar ist.

Daneben sammelt die Brücke, was die Seite von sich aus meldet – Konsolenfehler, unbehandelte Ausnahmen und über `Log.enable` fehlgeschlagene Anfragen. `CaptureResult::diagnostics` enthält die letzten 20 Meldungen des aktuellen Dokuments, älteste zuerst, mit dem Alter relativ zur Aufnahme.

## Prozess und Fenster

Der Daemon wählt einen freien Loopback-Port, startet die Shell (in einer systemd-Scope mit `MemoryMax=3G`, wenn es eine Benutzersitzung gibt) und wartet auf deren DevTools-Endpunkt. Das App-Panel erkennt er einmalig an seiner Adresse; dessen Target ist danach weder Tab noch für Agenten sichtbar. Chromiums eigenes Protokoll steht in `shell.log` im Sitzungsverzeichnis.

Schließt der Mensch das Fenster, meldet die Shell `closed`, bevor die Verbindung endet: `BrowserEvent::Closed`. Endet die Verbindung ohne diese Meldung: `BrowserEvent::Crashed`. Ein bewusster `stop()` meldet keins von beiden.

## Kontrollgrenze

Alle Proxybefehle prüfen Agent und Lease. FIFO je Tab verhindert vertauschte Maus-down/up-Befehle; eine gemeinsame Arbitration serialisiert auch mehrere Proxyverbindungen. Wartende Befehle tragen ihre Grant-Generation und werden nach Takeover oder erneutem Grant nie nachgeholt. Takeover widerruft sofort, meldet `settling`, wartet 500 ms und sendet `Page.stopLoading`. **Bereits laufendes JavaScript wird dadurch nicht beendet.**

Browserweite CDP-Methoden sind standardmäßig gesperrt. Discovery und Autoattach zeigen nur HTTP(S)- und leere Seiten-Targets, nie das App-Panel. `Target.attachToBrowserTarget` wird durch eine virtuelle, weiterhin gefilterte Browsersession ersetzt; fremde Targets, Browserkontexte und `Browser.close` sind gesperrt. `Target.createTarget` öffnet einen Tab im Fenster der Sitzung – Chromiums eigener Befehl würde ein fremdes Fenster außerhalb der App erzeugen – und der Proxy leitet das Attach-Ereignis der neuen Seite weiter, bevor er antwortet. `Browser.setDownloadBehavior` wird als No-op quittiert, weil Playwright es beim Verbinden braucht.

## Prüfen

```sh
cargo test -p zuruf-browser                                      # ohne Browser
ZURUF_SHELL=target/debug/zuruf-shell \
  cargo test -p zuruf-browser --test shell -- --ignored          # echtes Fenster, braucht einen Bildschirm
cargo run -p zuruf-browser --example session -- http://localhost:5173/   # von Hand
```
