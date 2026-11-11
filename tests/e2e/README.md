# Reale Verdrahtung – Abnahmebericht

> **Stand integrierter Browser.** Zuruf bringt sein Browserfenster selbst mit (`zuruf-shell`); System-Chromium, Extension und die Unit `zuruf-app.service` gibt es nicht mehr. Der aktuelle, T3-unabhängige Durchlauf ist `node tests/e2e/shell.mjs`: echter Daemon, eigenes Fenster, angedockte App, unverändertes Playwright als Agent über `/cdp`, Hotkey-Aufnahmen, Takeover, Absturz und Neustart, Fenster schließen. Er läuft unter X11 und Wayland, ohne Desktop im Container aus `tools/dev/`. Bauen: `pnpm --filter @zuruf/ui build`, `pnpm --filter @zuruf/page build`, dann `cargo build`. Der Bericht unten stammt aus der Zeit davor; seine Messwerte und die Abschnitte zu App-ID, Panel und Extension sind überholt.

Stand: 10.11.2026, Branch `feat/wire`. Kein vollständiges AT-01–AT-20-Abnahmeversprechen.

## Start und Build

Alle Befehle aus dem Worktree ausführen. UI vor Extension bauen, beide vor Rust (Bundles werden eingebettet):

```sh
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=1G -p MemorySwapMax=512M -- pnpm --dir apps/ui build
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=1G -p MemorySwapMax=512M -- pnpm --dir apps/page build
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- mise exec rust@stable -- cargo build -j 2 -p zuruf
```

Einmalig `zuruf t3 pair --origin http://127.0.0.1:<verifizierter-port>` in einer begrenzten Unit ausführen. Pairing benutzt den vorhandenen Pfad `t3 auth pairing create`. Es werden keine Tokens gedruckt. Der Origin wird privat gespeichert; alternativ explizit `T3_URL` setzen. Credentials bleiben bei Dienstneustarts erhalten. Transportfehler und HTTP401 erzeugen keine neue OAuth-Identität und keinen stillen Replay. Ein abgelaufenes Credential benötigt eine ausdrücklich vorgenommene Neupaarung; vor erneutem Versand offene Marker abgleichen. `ZURUF_T3_TRANSPORT=cli` wählt ausdrücklich den eingeschränkten CLI-Fallback; dieser bietet keinen Livekatalog und keine Attachments.

```sh
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- target/debug/zuruf daemon
# Alternativ startet open den Dienst (installierte User-Unit oder begrenzte transiente Unit):
target/debug/zuruf open --view sessions
target/debug/zuruf status
target/debug/zuruf stop
```

Chromium für Reviews läuft in einer eigenen 3-GiB-Scope; das App-Fenster in `zuruf-app.service` mit 1 GiB. `stop` beendet auch diese App-Unit. Keine Benutzerprofile werden benutzt. `ZURUF_FAKE_ADAPTERS=1` bleibt verfügbar; dort druckt `open` für bestehende Tests nur den Einmalcode-Link.

## Wiederholbare Liveprüfung

```sh
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=2G -p MemorySwapMax=512M -- env T3_URL=http://127.0.0.1:<verifizierter-port> node tests/e2e/wiring.mjs
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=768M -p MemorySwapMax=256M -- node tests/e2e/app-smoke.mjs --allow-generated-id
```

`wiring.mjs` erzeugt genau einen echten T3-Thread pro Aufruf. Projektstandard ist der bereits registrierte Zuruf-Projektroot; überschreibbar mit `ZURUF_E2E_PROJECT`. Der Provider erhält ausschließlich den Wire-Worktree als bestehenden Workspace. Der Test verbietet Dateiänderungen. Credentials werden unter `~/.local/state/zuruf` wiederverwendet (`ZURUF_E2E_CREDENTIAL_STATE` überschreibt diesen Pfad). Testprofile/Daten liegen unter `~/.local/share/zuruf/e2e/<run>/`; nur geheimnisfreie Messwerte liegen unter `.spike/wiring/`. Die Fixture bindet an 127.0.0.1. Der Test beendet Browser, Fixture, seinen Daemon und noch laufende Testtasks im finally. Ein Fehler des finally wird nicht als erfolgreiche Bereinigung gewertet; Units anschließend nachprüfen.

`--capture-only` prüft zusätzlich schnelle W/D-Folgen ohne einen weiteren T3-Thread zu erzeugen. Dieser Lauf wurde nach der abschließenden Isolation der Capture-Envelopes erfolgreich wiederholt.

Der Test nutzt echte PipeWire-Ambientaufnahme und ausdrücklich `base.en`/`en`, weil nur dieses Modell installiert ist. Produktionsstandard bleibt `de` mit multilingualem `base`; ein fehlendes Modell erzeugt `speech.model_missing` und HTTP-Fehler. Kein Modelldownload, keine Voxtype-Konfigurationsänderung und kein Voxtype-Neustart.

## Automatische Checks

Abschließend erfolgreich: `cargo test --workspace -j 2` (103 Tests, fünf ausdrücklich ignorierte Liveprüfungen), `cargo clippy --workspace --all-targets -j 2 -- -D warnings`, `cargo fmt --all --check`, Extension `check`, `lint`, `build` und UI `build`, sämtlich in begrenzten systemd-Units. App-Autostart samt tatsächlich sichtbarem Fenster und dessen PID wurde mit `--allow-generated-id` geprüft; der strenge Identitätstest ohne Ausnahmeparameter schlägt aus dem unten dokumentierten Grund fehl.

## Ergebnisse

Zwei reale headed Durchläufe ohne Fakeflag waren erfolgreich: API-Sessionstart, authentifizierter Extension-WS, W/S/D mit persistierten Raw-/Marked-Bildern, Continuous-Aufnahme, Shift+N, neuer aktiver Draft, verspätetes Segment ausschließlich im versiegelten Batch, T3-Launch mit drei hochgeladenen Marked-PNGs, Statusabfrage, Takeover während eines 900-ms-Evaluate und Ablehnung nachfolgender Befehle. Lauf zwei wartete zusätzlich bis `needs_review` und unterbrach den Testtask danach. Beide Testthreads sind remote `interrupted`; keine Testprovider laufen weiter.

| Messung | Lauf 1 | Lauf 2 |
|---|---:|---:|
| W einschließlich Persistenz | 288 ms | 285 ms |
| S einschließlich Auswahl und Persistenz | 264 ms | 288 ms |
| D einschließlich Persistenz | 237 ms | 257 ms |
| Shift+N bis neuer Draft beobachtbar | 64 ms | 62 ms |
| Takeover einschließlich HTTP | 516 ms | 518 ms |

Das Captureziel <150 ms wird verfehlt. Die Messungen enthalten Automations-/HTTP-Polling, bei S zusätzlich den Drag. Keine Behauptung akustisch latenzfreier Hardwaremarker. Fünf alte Segmente blieben je Durchlauf beim alten Batch; im zweiten Lauf wurde ausdrücklich ein erst nach dem Wechsel fertiger Abschnitt geprüft. Die zusätzlichen PCM-Grenzen werden ohne Warten auf ASR gesetzt.

Erstellt wurden ausschließlich:

- [zuruf-test-wiring-1794331569135](https://app.t3.codes/threads/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp%3Alaunch%3A5c063c6ee5e36789d17ce8cb907054750c24b153c68a8f14552b8ba15ade9b42), ID `mcp:launch:5c063c6ee5e36789d17ce8cb907054750c24b153c68a8f14552b8ba15ade9b42`, Titelzusatz `[zuruf:01a1c67e-fbc5-74d1-8f10-ab2b0e978611] [01a1c67e-fbc5-74d1-8f10-ab2b0e978611]`.
- [zuruf-test-wiring-1794331786267](https://app.t3.codes/threads/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp%3Alaunch%3Adf75b4bb1c17719eded82330373f54a8bab7eb436f7e0ca1cd40217737ebf018), ID `mcp:launch:df75b4bb1c17719eded82330373f54a8bab7eb436f7e0ca1cd40217737ebf018`, Titelzusatz `[zuruf:01a1c682-477d-7660-85ce-970837f255e3] [01a1c682-477d-7660-85ce-970837f255e3]`.

## Abweichungen, Grenzen und offene Abnahme

- **App-ID:** Autostart, eigenes Profil, Wayland und chromeless Fenster funktionieren. Der strenge `app-smoke.mjs` ohne Ausnahmeparameter schlägt jedoch fehl: Chromium setzt bei `--app` trotz `--class=zuruf` tatsächlich `chrome-127.0.0.1__-Default`. Der Installer muss vorläufig diese beobachtete ID matchen; eine exakte App-ID `zuruf` ist ungelöst. Die Appfensteridentität wird aus Appname/Profil generiert ([Chromium-Quellcode](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/frame/browser_native_widget_aura_linux.cc)). Keine erfundenen Flags, kein Browserpatch. Wiederholtes `open` bei bereits aktiver App startet keine zweite Instanz; Fokus und nachträglicher Viewwechsel sind noch offen.
- **Capture-Recovery:** Unterbrochene Platzhalter bleiben erhalten; zugehörige noch nicht versandte Tasks werden beim Neustart sofort sichtbar fehlgeschlagen. Automatische Wiederaufnahme ist weiterhin nicht möglich. Raw-PNG und Marked-PNG kommen erst zusammen aus der Browserbibliothek; frühe Raw-Persistenz vor Komposition ist nicht implementiert. Bestätigte Bilder und Metadaten werden weiterhin atomar/transactional persistiert.
- **Sprache:** Continuous ist Standard, samplegenaue Grenzen innerhalb des empfangenen PCM. PipeWire-/Hardwarepufferung bleibt. Daemon-Fallback meldet Warnung, verzögerte Grenzen und fehlende Sprachüberschreibung; kein Waveformsignal vom Daemon-Fallback. Ein Modell-/Backendwechsel während Aufnahme wirkt beim nächsten Start. Modellprüfung sucht im üblichen lokalen Voxtype-Modellverzeichnis; alternative Modellpfade sind nicht integriert. Absturz unterbricht ausstehende ASR-Futures sichtbar; kein WAV-Recovery. Deutsche Stimme, drei unterschiedliche gesprochene Kommentare und Sprache bei aktivem Gerätetausch sind nicht abgenommen.
- **T3:** Maximal acht Marked-Bilder pro Start/Nachricht (reales Serverschema); größere Batches werden ausdrücklich abgewiesen. Attachment-Follow-ups sind verdrahtet, aber in dieser Welle nicht separat live geprüft. Der Attachment-Send-Endpunkt hat keinen Replaykey; mehrdeutiger Ausgang bleibt `unknown_remote_state`. Uploads vor einem fehlgeschlagenen Launch werden noch nicht verworfen. Status wird mit dauerhaftem MCP-HTTP-Client gepollt; die vorhandene Watch-/Backoff-API wird nicht verwendet. Ein Mutex serialisiert einzelne T3-Requests, nicht die Aufnahmepipeline. Zwei gleichzeitige reale Providerläufe (AT-11), Replay mit verlorener Launchantwort und manuelle Approvalbeantwortung wurden in dieser Welle nicht live geprüft; vorhandene Fake-/Adaptertests bleiben erhalten.
- **Tabs:** CDP-Target-ID → UUID wird privat persistiert. Gleichzeitige Hotkeyaktionen verschiedener Browser-Sessions verwenden weiterhin die globale aktive Review-/Sessionauswahl; diese Cross-Session-Race ist noch nicht abgenommen. Nach Browserrestart neue Targets erhalten neue UUIDs; gleiche URLs werden nicht als identische Tabs geraten. Letzte gemeldete, redigierte URLs werden mit dem Snapshot persistiert. Keine Garantie für Navigation unmittelbar vor Crash ohne Extensionmeldung.
- **Proxy:** Leaseablauf wird monoton erzwungen; ein eigenes Ablaufereignis für die Anzeige fehlt noch (die nächste Kontrolländerung aktualisiert sie). Bestehende Browsergrenzen bleiben: bereits laufendes JavaScript kann nach Takeover weiterwirken; ungeschützter roher CDP-Port ist nur für lokale Prozesse erreichbar, keine OS-Sandbox. Vollständige Playwright-Kompatibilität, Cross-Origin-/verschachtelte Frames und Sonderseiten bleiben wie im Browser-README begrenzt. Agenttokens sind ephemer; nach Restart neu ausstellen.
- **Desktop:** Live-Rebinding und Konfliktprüfung sind verdrahtet; Reloadbehandlung ist implementiert, aber in dieser Welle kein Compositorreload ausgelöst. Ein Socketwechsel ohne nachfolgendes Reloadereignis erfordert noch erneutes Anwenden der Einstellungen. Physische Tastatur, reale Editoren/IME, Login/MFA, Cookiepersistenz und visuelle Panelabnahme bleiben menschlich beziehungsweise den bisherigen Browserprüfungen vorbehalten.
- **Wire-Erweiterungen:** zusätzlich zu bestehenden Routen `POST /api/v1/shutdown`, `POST /api/v1/sessions/<id>/restart`, `POST /api/v1/sessions/<id>/agent-access`. Letztere benötigt das Daemon-Bearer, aktiviertes `agentAccess`, `{agentId}` und liefert ein geheimes Agenttoken; `{revoke:true}` widerruft es. Keine Agenten-Bearer in URL/Manifest/Log. Frühe App-Smokes nutzten noch systemd-Standardbeschreibungen, die kurzlebige Einmalcode-URLs ins Journal übernahmen; diese Codes sind verbraucht/abgelaufen. Explizite Unitbeschreibungen verhindern das in der endgültigen Implementierung. Langfristige Bearer-/OAuth-Tokens wurden nicht geloggt. `/cdp/<id>` verlangt dieses eigene Bearer und eine explizite Kontrolllease; `control.revoke`, Tokenrotation und Sessionstop widerrufen aktive Zugangsklone. `/ext` prüft den exakten eingebauten Extension-Origin und das erste Authframe gegen einen aktiven Sessionhub. Panel-HTTP/Events akzeptieren das Sessiontoken; CORS ausschließlich für den exakten eingebauten Extension-Origin. `packages/protocol` wurde nicht geändert; Routen sind mit dem Projektleiter abzugleichen.
- **MCP-Mount:** kein `zuruf-mcp` angelegt, keine Workspace-Memberliste geändert (`crates/*`). `Daemon::router()` hat explizite Routen und eine finale UI-Fallbackroute; der parallele MCP-Agent kann vor dem Fallback `.merge(...)` ergänzen. Adapterglue liegt separat in `real.rs`.

Keine Änderungen an UI-Komponenten oder `apps/ui/src/lib/**`, kein Overlay-Restyling. Keine Installation, kein Push und kein PR. T3-Dateien/Datenbank, echte Browserprofile, Voxtype-Konfiguration und Hyprland-Konfiguration blieben unberührt. Runtimebindungen wurden nach Tests auf null geprüft.

## Welle 3

Der historische Welle-2-Bericht oben bleibt als Übergabenachweis erhalten. Aktueller Stand und Handabnahme: [docs/abnahme.md](../../docs/abnahme.md), aktuelle Bedienung: [Benutzerhandbuch](../../docs/benutzerhandbuch.md).

`bash tests/e2e/run.sh` führt die sechs unabhängigen Szenarien jeweils unter 3 GiB/1 GiB Swap aus und meldet PASS/FAIL samt Logpfad. Vorher UI → Extension → Rust inklusive `cargo build -p zuruf-browser --example lab -j 2` bauen sowie `npm --prefix tests/mcp ci` ausführen. Keine realen Agentenstarts im Standardlauf. Die Captureprüfung setzt den bereits gepaarten lokalen T3-Zugang voraus, startet aber keinen Thread. Sie misst Keyevent/Bestätigungs-RAF direkt in der Extension und Rohbildereignisse über WebSocket ohne Statuspolling als Zeitquelle. Die sonstige Ablaufsteuerung darf weiterhin pollen.

`wiring.mjs --completion` ist ausdrücklich ein Nutzer-Handabnahmelauf: zwei echte Threads, nötige T3-Freigaben nur durch den Nutzer; bei ausstehender Freigabe scheitert der automatische Teil und räumt auf. Erteilt kein Agent selbst. `followup.mjs` verwendet einen bestehenden synthetischen Task (`ZURUF_FOLLOWUP_RUN`), erzeugt keinen weiteren Thread und unterbricht den Testlauf anschließend. Niemals einen produktiven Task dafür verwenden.

## Leere Erstinstallation

`firstrun.mjs` startet einen echten Daemon auf Port 47643 mit drei leeren temporären Datenverzeichnissen. Er verwendet keine installierte Zuruf-Kopie und kopiert keine bestehenden Credentials. Er prüft Pairing ohne Origin (verifiziertes lokales T3 auf 3773), wiederholtes Pairing, CLI-Livepaarung, Reconnect, Livekatalog, automatische Projekt-/Review-/Draftanlage, erstes Profil, headed Chromium mit Basis-URL, W-Capture, Dispatch eines schreibgeschützten Testauftrags, Interrupt, Projektwechsel und Browserstop. Playwright bedient den verwalteten Browser über den öffentlichen, authentifizierten CDP-Proxy. Kein direkter Storezugriff oder Seed. Die zusätzliche OAuth-Session wird im finally über den verifizierten Befehl `t3 auth session revoke` widerrufen. Ihre ID und Threadlinks werden ohne Secrets ausgegeben. Bei fehlgeschlagener Bereinigung bleibt das temporäre Verzeichnis zur Diagnose erhalten.

```sh
systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- node tests/e2e/firstrun.mjs
# Optional im Gesamtlauf (erzeugt einen echten T3-Thread):
ZURUF_E2E_FIRSTRUN=1 bash tests/e2e/run.sh
```

`T3_URL` kann für den Test einen expliziten lokalen Origin vorgeben; der Testdaemon erhält diese Variable nicht und muss zuerst unverbunden melden. Voraussetzung ist das registrierte Zuruf-Projekt `/home/mika/Projects/Humaify/zuruf/main` und ein aktivierter Codex-Provider im Livekatalog. Alle Server binden ausschließlich Loopback. Keine Audioaufnahme in diesem Szenario.
