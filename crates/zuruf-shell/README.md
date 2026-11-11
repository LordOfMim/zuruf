# zuruf-shell

Zurufs eigenes Browserfenster: eingebettetes Chromium (CEF über das Crate `cef`) mit der App links und der Seite rechts. Ein eigenes Programm, damit nur dieses gegen Chromium gelinkt ist und die CLI `zuruf` klein bleibt. Der Daemon startet je Browser-Sitzung eine Shell; von Hand startet man sie nur zum Entwickeln.

```text
┌──────────────┬─────────────────────────────────────────┐
│              │ Tab 1 │ Tab 2 │ +                       │
│  App-Panel   ├─────────────────────────────────────────┤
│ (--zuruf-    │ ← → ↻  Adresse                    Breite │
│   panel)     ├─────────────────────────────────────────┤
│              │              sichtbare Seite            │
└──────────────┴─────────────────────────────────────────┘
```

## Schalter

| Schalter | Bedeutung |
|---|---|
| `--zuruf-profile=<Verzeichnis>` | Browserprofil der Sitzung |
| `--zuruf-panel=<Adresse>` | Seite der App; ohne sie ist das Fenster ein schlichter Browser |
| `--zuruf-panel-width=<px>` | Breite des Panels, Standard 460 |
| `--zuruf-app-only` | nur die App, fensterfüllend, keine Tabs |
| `--zuruf-url=<Adresse>` | Tab öffnen; mehrfach möglich |
| `--zuruf-port=<Port>` | DevTools-Endpunkt auf Loopback; ohne Angabe gibt es keinen |
| `--zuruf-theme=system\|dark\|light` | Farben der Fensterelemente |
| `--zuruf-page-width=<px>` | Seiten mittig in dieser Breite zeigen statt in voller Breite |
| `--zuruf-detached` | weiterlaufen, wenn stdin endet (Handstart, Tests) |

Alle anderen Schalter gehen an Chromium, zum Beispiel `--no-sandbox --disable-gpu` in einem Container. In einer Wayland-Sitzung wählt die Shell selbst `--ozone-platform=wayland`.

## Befehle und Ereignisse

Eine JSON-Zeile je Befehl auf stdin:

```json
{"command":"open","url":"http://localhost:5173/"}
{"command":"focus"}
{"command":"panel","visible":true}
{"command":"theme","mode":"dark"}
{"command":"page-width","width":390}
{"command":"quit"}
```

`panel` ohne `visible` schaltet um, `page-width` ohne `width` stellt die volle Breite wieder her. Auf stdout meldet die Shell `{"event":"ready"}`, sobald das Fenster steht, und `{"event":"closed"}`, wenn es geschlossen wird. Endet stdin, beendet sich die Shell – ein Fenster ohne Daemon hat keinen Zweck.

## Aufbau

- `main.rs` – Prozessstart. Chromium startet dasselbe Programm für seine Hilfsprozesse erneut.
- `options.rs` – Schalter und die Deutung getippter Adressen.
- `shell.rs` – Fenster, Tabs, Thema, Kurzbefehle. Der Zustand liegt in einer thread-lokalen Zelle und wird nie über einen Aufruf in CEF hinweg gehalten, weil solche Aufrufe direkt in die eigenen Handler zurückkommen können.
- `handlers.rs` – Chromiums Rückrufe, weitergereicht an `shell`.
- `commands.rs` – stdin und stdout.

Drei Eigenheiten, die man kennen muss: Eigene Hintergrundfarben von Views setzt Chromium bei jedem Themenwechsel zurück und meldet das, während es noch dabei ist – `shell::theme_changed` färbt deshalb in einer nachgestellten Aufgabe neu. Die beiden anderen betreffen die Rust-Bindings: Eine nicht beantwortete Rückfrage bedeutet „nein“ (anders als in CEFs C++-Vorgaben) – ein Fenster, das `can_resize` nicht beantwortet, ist fest und wird von Tiling-Compositoren schwebend gesetzt. Und Zeichenketten in Strukturen, die CEF nach dem Rückruf liest, müssen als rohe CEF-Zeichenkette übergeben werden (`handed_over` in `handlers.rs`), sonst kommen sie leer an.

## Bauen und ausprobieren

Der Build braucht `cmake` und `ninja` und lädt beim ersten Mal die Chromium-Laufzeit; `CEF_PATH` legt fest, wohin. Zum Start müssen `libcef.so` und Chromiums Ressourcen neben dem Programm liegen: Im Target-Verzeichnis legt der Build sie selbst dorthin, für die Installation erledigt es `install.sh`.

```sh
tools/dev/run-shell.sh --zuruf-url=http://127.0.0.1:8700/index.html      # X11 unter Xvfb
su dev -c 'bash tools/dev/run-wayland.sh --zuruf-url=about:blank'         # Wayland unter sway
```
