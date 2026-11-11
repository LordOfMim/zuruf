# Zuruf – Abnahme, Welle 3

> **Nachtrag 11.11.2026 – integrierter Browser.** Zuruf läuft seither in einem eigenen Browserfenster (`zuruf-shell`, eingebettetes Chromium 156) statt in System-Chromium mit Extension. Der Bericht unten beschreibt den Aufbau davor; alles zu App-ID, Seitenleiste, Extension und die gemessenen Latenzen gilt für den neuen Aufbau nicht als belegt.
>
> Neu geprüft, ohne Desktop in einem Linux-Container (X11 unter Xvfb und Wayland unter sway, Software-Rendering, ohne Sandbox): `cargo test --workspace` (alle bestanden), `cargo test -p zuruf-browser --test shell -- --ignored` (Sitzung im echten Fenster), die Tastenprüfung des Seiten-Agenten (7 von 7), `tests/e2e/shell.mjs` (X11: 11 Prüfungen, Wayland: 12 einschließlich „Fenster schließen“) mit unverändertem Playwright als Agent über `/cdp`, sowie der Installationsrundlauf mit dem Release-Build. Gemessen am installierten Release-Build im Container: `zuruf open` kehrt nach rund 0,2 s zurück, eine Seite nimmt rund 0,4 s nach Sitzungsstart Hotkeys an, ein Fenster mit App und einer Seite belegt rund 310 MiB, der Daemon 13 MiB.
>
> **Nicht geprüft und auf dem Zielrechner nachzuholen:** Hyprland selbst (Fokus über `hyprctl`, globale Hotkeys, Fensterregeln), GPU-Rendering und die Chromium-Sandbox, die systemd-Scope mit Speichergrenze, HiDPI und Browserzoom (die Zoom-/DPR-Matrix unten), physische Tastatur und IME, Sprache mit Voxtype, T3-Anbindung, Anmeldungen mit 2FA sowie die Latenz vom Tastendruck bis zur Bestätigung.

Stand: 10.11.2026. Checkout `worktrees/finish`, Branch `feat/finish`. Kein vollständiges Produktabnahmeversprechen. Die Ergebnisse beziehen sich auf diesen Rechner (Hyprland/Wayland, Chromium 155, Voxtype 1.1.1, T3 0.1.7, nur `base.en`). Keine Nutzerfreigabe in T3 wurde stellvertretend erteilt.

## AT-01 bis AT-20

„Bestanden“ gilt für den konkret genannten ausgeführten Testumfang. Synthetische Tests beweisen keine beliebigen Kundenseiten, menschliche MFA oder deutsche Sprachqualität. Offene Erweiterungen stehen ausdrücklich dabei.

| AT | Test / Handverfahren | Ergebnis | Nachweis / Grenze |
|---|---|---|---|
| AT-01 | Menschlich anmelden, MFA selbst durchführen, Session stoppen/starten und erneut öffnen | manuell offen | Synthetisches langlebiges Cookie besteht im headed Labor; echter Login/MFA wurde nicht ausgeführt. |
| AT-02 | `target/debug/examples/lab --check`: Shared-Target-ID und Klick über CDP/Playwright-MCP | bestanden | `.spike/acceptance/headed-browser.log`, `.spike/browser-lab/evidence.json`; derselbe sichtbare Tab. |
| AT-03 | Headed Labor: Takeover während laufendem Timer, danach neuen Befehl ablehnen, Release | bestanden | Headed-Log; bereits gestartetes JavaScript läuft ggf. weiter, kein Rollback. |
| AT-04 | Headed Labor: frisches isoliertes Profil ohne vorheriges Cookie | bestanden | Headed-Log; Profile nacheinander gestartet, keine unbeabsichtigte Cookieübernahme. |
| AT-05 | Headed Labor + `wiring.mjs --capture-only`: W, PNG/Ring/Koordinaten/Metadaten | bestanden | Headed- und Capture-Logs, persistierte Raw-/Marked-Assets. Latenzziel separat unten. |
| AT-06 | Headed Labor: Region bei echtem Browserzoom 100/125 %, emuliertem DPR 1/2 pixelweise vergleichen | bestanden | 80×40, 100×50, 160×80, 200×100 Pixel; echte Mischskalierungsmonitore bleiben offen. |
| AT-07 | Headed Labor: D, AX-Rolle/Name, Shadow Root, Same-Origin-Frame und Locator | bestanden | Headed-Log/Browser-Evidence; OOPIF/geschlossene Roots nicht vollständig abgedeckt. |
| AT-08 | Deutsches Diktat mit vollständigem Dateieinlesen und eindeutiger Zuordnung | manuell offen | Englische Ambient-/PCM-Pipeline läuft real. Seit 10.11.2026 folgt Zuruf ohne eigenes Modell der Voxtype-Konfiguration (hier Remote-Endpunkt); CLI-Aufruf mit Dienst-Umgebung real geprüft, gesprochenes deutsches Diktat in der App noch von Hand offen. |
| AT-09 | Drei unterschiedliche gesprochene Kommentare, W/S/D, anschließend einzeln korrigieren | manuell offen | Markerlogik automatisiert; verständliche deutsche Kommentare nicht gesprochen/geprüft. |
| AT-10 | `at10_at11_parallel_dispatch`, real `wiring.mjs --completion`: Shift+N, neuer Draft während T3 läuft | bestanden | Workspace-Log; realer neuer Draft nach 71,44 ms im Parallelitätslauf (Polling-Obergrenze). |
| AT-11 | Zwei reale Dispatches, bevor der erste Agent endet, beide Boardtasks laufen | bestanden | `tests/e2e/evidence-completion.json`: beide Status `running`; später warteten beide auf Nutzerfreigabe und wurden unterbrochen. |
| AT-12 | Reales Launchreceipt und T3-Thread lesen: ausgewähltes Modell/Provider | bestanden | Beide Threads: `codex`, `gpt-6-luna`, `approval-required`, richtiger Finish-Worktree. Full-access-Anforderung wurde zuvor abgewiesen. |
| AT-13 | Nutzer-Handverfahren unten: realer Agent liest Manifest und PNG über Zuruf-MCP und meldet Fortschritt | manuell offen | SDK-Prüfung besteht; reale lokale Lesebefehle benötigen T3-Nutzerfreigabe. Keine Freigabe erteilt. |
| AT-14 | `at14_late_transcript_owned_by_old_batch`, realer verzögerter PCM-Abschnitt nach Shift+N | bestanden | Workspace-Log und Completionlauf vor Freigabeblockade; Abschnitt bleibt im alten Batch. |
| AT-15 | `at15_ambiguous_requires_resolution`, `at15_failure_retry_no_duplicate`, `at15_receipt_lost_adopts_marker`; zusätzlich reale Launchantwort gezielt verwerfen | manuell offen | Fake-Logik besteht, realer Response-Drop wurde nicht ausgeführt. Abgewiesener Launch blieb ohne automatischen Replay unklar; keine Thread-ID. |
| AT-16 | Zwei tatsächlich schreibende Aufträge in getrennten Worktrees; Diffs auf Überschneidung prüfen | manuell offen | Konflikt-/Workspaceprüfungen automatisiert; reale Tests verbieten Dateiänderungen und schreiben nicht. |
| AT-17 | Headed Labor: input/textarea/password/contenteditable/Shadow/iframe/select/Editor/Custom-Element | bestanden | Headed-Log; physische Tastaturen, IME und reale Editorprodukte bleiben Handprüfung. |
| AT-18 | `at18_restart_persists_all_local_context`, `restart_during_running_tracks_same_run`, Store-Transaktionstests | bestanden | Workspace-Log: lokale Assets/IDs/Receipts bleiben erhalten und gleicher Run wird weiter beobachtet. Echter Daemonkill mitten in einem Providerlauf bleibt zusätzliche Handprüfung. |
| AT-19 | Metadaten-/PNG-Maskentests; anschließend alle sichtbaren sensitiven Inhalte und Exporte menschlich prüfen | manuell offen | Allowlist, URL-/Inputredaktion und skalierte schwarze Masken automatisiert geprüft. Beliebige sichtbare Texte/Bilder und fremde Frames nicht semantisch anonymisiert. |
| AT-20 | Externe CLI-/MCP-Schnittstellen, Diff und Laufzeitprüfung | bestanden | Keine T3-Source-/DB-/Providerdatei verändert; alle Launches/Uploads/Interrupts über externe Schnittstellen. |

Matrix: **13 bestanden, 0 fehlgeschlagen, 7 manuell offen** (AT-01, 08, 09, 13, 15, 16, 19). Das zusätzliche nichtfunktionale Captureziel ist mit der dokumentierten Beobachtungsgrenze nicht nachgewiesen. Keine offenen Handprüfungen werden durch Fake-Ergebnisse ersetzt.

## Exaktes Handverfahren für den realen MCP-Nachweis

1. Den endgültigen Build mit UI → Extension → Rust erstellen; den Daemon mit privatem Zustandsverzeichnis und ausdrücklich gepaartem T3-Origin starten. Keine Credentials in `.env`, Manifest oder Repository ablegen. `approval-required` verwenden, nicht zur Umgehung einer Freigabe auf einen breiteren Modus wechseln.
2. Synthetisches Projekt/Review und eine Session mit Agentenzugriff anlegen. Einen W-Capture vollständig speichern. Batchtitel mit `zuruf-test-` beginnen. Batchanweisung: „Transporttest. Keine Dateien ändern. Lies das Batchmanifest und ein Screenshotasset über den bereitgestellten Zuruf-MCP-Zugang. Melde anschließend `MCP_PROOF_OK` über `report_progress`. Gib niemals das Credential aus.“ Per Shift+N einen echten Thread starten.
3. Der Prompt enthält den Manifestpfad und den Pfad `<state>/agents/<task>/credential.json`, ausschließlich als Pfad. Die Datei muss Modus 0600 haben; sie enthält `url`, `origin`, `token`. Der Agent muss sie lokal lesen und mit den Werten HTTP-JSON-RPC an `/mcp` senden. Nicht `cat` auf die Credentialdatei ausführen oder Tokens in Shellargumente einsetzen.
4. In T3 erscheint eine Freigabe für den lokalen Shell-/Python-Lesebefehl. Im tatsächlich beobachteten Lauf lautete sie sinngemäß `pwd && rg --files ... && python ...`; das Pythonprogramm öffnete ausschließlich die taskprivate Credentialdatei und gab nur Feldnamen, URL und vorhandene Headernamen aus. Der zweite Testthread verlangte `pwd && rg --files ... && cat <batch.json>`. **Nur der Nutzer prüft und genehmigt diese konkreten Befehle in T3.** Befehle, die Credentialwerte ausgeben oder Dateien ändern, nicht freigeben; den Agenten korrigieren.
5. Weitere lokale Datei-/HTTP-Befehle ebenfalls nur durch den Nutzer freigeben. Prüfen: `initialize`, `tools/call` mit `get_batch_manifest`, `list_annotations`, `get_annotation_asset` mit einer Asset-ID dieses Batches und `report_progress {message:"MCP_PROOF_OK"}`. Der Screenshot muss als MCP-Bildinhalt beim Agenten ankommen; kein bloßes lokales Öffnen der Datei als Ersatz. Fortschrittsmeldung muss im Zuruf-Taskevent erscheinen.
6. Den Run unterbrechen oder beenden. Prüfen: Datei entfernt, altes Bearer liefert 401; CDP mit altem Bearer/Lease wird abgewiesen. Threadtitel/ID, Toolnamen, Bildabmessungen und Progressnachricht geheimnisfrei dokumentieren. Keine Header-/Credentialbodies protokollieren.

Der automatisierte Completionlauf räumt bei ausstehender Freigabe auf. Eine vollständige Handprüfung braucht deshalb eine normal gestartete Session oder einen bewusst vom Nutzer begleiteten Lauf mit entsprechendem Zeitfenster.

## Reale Threads dieser Welle

Beide neuen Threads sind nach der ausdrücklichen Nutzeranweisung unterbrochen worden; es gab keine stellvertretende Freigabe. Der erste wurde später für einen trivialen bildtragenden Follow-up wiederverwendet, antwortete ohne Toolaufruf `FOLLOWUP_OK` und ist danach abgeschlossen (`completed`, kein aktiver Run). Der zweite Thread bleibt `interrupted`. Kein zusätzlicher Thread für diesen Follow-up.

- Titel `zuruf-test-wiring-1794332745486 [zuruf:01a1c690-eb06-72a1-a140-21e91b7d5075] [01a1c690-eb06-72a1-a140-21e91b7d5075]`; ID `mcp:launch:2f11866463f5a06ca01ba57facd3c1d2e52b52e9120329a9e3b581c13223b69f`; <t3-thread://v2/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp:launch:2f11866463f5a06ca01ba57facd3c1d2e52b52e9120329a9e3b581c13223b69f>
- Titel `zuruf-test-concurrent-1794332745486 [zuruf:01a1c690-f787-713e-8e7d-8ed991d0cf6d] [01a1c690-f787-713e-8e7d-8ed991d0cf6d]`; ID `mcp:launch:db5f4ea2a1b12fd9caa0e942147d2a54329597baeaf2b3a1a7d7b11fd2d5698e`; <t3-thread://v2/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp:launch:db5f4ea2a1b12fd9caa0e942147d2a54329597baeaf2b3a1a7d7b11fd2d5698e>

Der frühere Versuch `zuruf-test-wiring-1794332658704` mit `full-access` wurde vom Server abgewiesen und lieferte keine Thread-ID. Es wurde kein automatischer Retry ausgelöst.

## Verträge, Restgrenzen und Nachweise

Neu: `ControlRequest.action=renew` (Rust und `packages/protocol/src/index.ts`); `POST /api/v1/app/open {view}` und `app.open_requested` (Rust/TS). Keine Protokollversionsänderung für diese additiven Erweiterungen. Zweistufige interne Browser-API `capture_raw`/`capture_request_raw`, `BrowserController::complete_capture`/`confirm_capture`. Keine zusätzliche öffentliche Captureroute.

UI-Funktion geändert ausschließlich in `apps/ui/src/lib/store.ts`: Viewwechsel per Event und Reload. Keine Komponenten-/CSS-Umgestaltung. Favicon über `apps/ui/index.html`/`public/zuruf.svg`. Fenster-/Launchericon wird über Zuruf-Desktopdatei und unsichtbaren Alias der beobachteten Chromium-ID zugeordnet; visuelle Prüfung der Desktopdarstellung bleibt offen.

Weitere Handprüfungen: echter Daemonkill/Crash mitten im Capture und Task; automatisches Rekonstruieren verlorener Captureoperationen nicht implementiert (Eintrag bleibt kommentiert erhalten, unvollständiger Versand blockiert); Mehrsession-Hotkey-Stresstest; tatsächlicher Compositorreload/-socketwechsel (Reapply implementiert); Agenten-Neuanmeldung nach Daemonrestart; Drag/Navigation während Takeover; MV3-Schlaf/Resume; OOPIF-/geschlossene Shadow Roots; reale Monitorskalierung; unscriptbare Sonderseiten; deutsche Sprache und echte MFA. Bekannte Raw-CDP-/laufendes-JavaScript-Grenzen stehen im Benutzerhandbuch.

Uploads werden vor bekannt fehlgeschlagenem Launch soweit die verifizierte Pending-Upload-Schnittstelle es erlaubt verworfen. Bei mehrdeutigem Launch bleibt der Remotezustand unklar, da Attachments bereits verbraucht sein können. Fehlerbereinigung und verlorene Antworten wurden nicht zusätzlich live erzwungen.

## Einordnung der acht Arbeitspakete

| Paket | Stand | Verbleibend |
|---|---|---|
| 1 Agentenzugriff | teilweise | Dateihandoff, gemeinsame MCP/CDP-Registry, Widerruf, atomare Renew und Ablaufereignis implementiert/automatisiert geprüft. Echter Agenten-MCP-Abruf braucht Nutzerfreigabe; Wiederanmeldung nach Restart offen. |
| 2 Latenz | teilweise | Zweistufige API, frühe Persistenz/Bestätigung und ereignisbasierte Messung vorhanden; Ziel <150 ms verfehlt. |
| 3 Appfenster | teilweise | Fokus und PID-Wiederverwendung live bestanden; Viewevent implementiert. View-/Icondarstellung im Desktop visuell noch manuell prüfen. Keine erfundene Chromium-App-ID. |
| 4 Recovery | teilweise | Unterbrochene Captures sichtbar, unsicherer Versand blockiert; Pending-Upload-Cleanup, Quellsessionbindung und Socket-Reconnect implementiert. Restart-/Replaylogik per Fake geprüft; reale Crash-/Drop-/Mehrsession-/Reload-Stresstests offen. Attachment-Follow-up live bestanden. |
| 5 Datenschutz | teilweise | Metadatenredaktion und konfigurierbare Masken im Hauptdokument; Freitext-/Bildsichtprüfung und komplexe Frames bleiben Nutzeraufgabe. |
| 6 Abnahmematrix | erledigt | Alle 20 AT erfasst, sechs Szenarien im Runner; echte AT-11-Parallelität ausgeführt, offene Kriterien ausdrücklich markiert. |
| 7 Installationsrundlauf | erledigt | Wegwerfpräfix, Upgrade, installierte CLI/App, tatsächlicher transienter Daemonstart mit installiertem Binary, Doctor, Unitsyntax, Starter, Datenerhalt und Purge. Keine Installation/Unitaktivierung im echten Home. |
| 8 Dokumentation | erledigt | Benutzerhandbuch, MCP-Vertrag und `.env.example` aktualisiert. |

Beobachteter Freigabebefehl des ersten Threads (historischer Nachweis; Datei nach Unterbrechung entfernt, nicht erneut ausführen):

```sh
pwd && rg --files -g 'AGENTS.md' -g 'docs/spec.md' -g 'docs/feasibility.md' -g 'docs/architecture.md' -g '!node_modules' -g '!target' | head -80 && python - <<'PY'
import json, urllib.request
p = '/home/mika/.local/share/zuruf/e2e/1794332745486/state/agents/01a1c690-eb06-72a1-a140-21e91b7d5075/credential.json'
c = json.load(open(p))
print('credential-fields:', sorted(c.keys()))
print('url:', c.get('url'))
print('headers-present:', [k for k in ('Origin', 'Authorization') if k in c or k.lower() in c])
PY
```

Der Befehl erzeugte eine T3-Freigabe für `/usr/bin/bash -lc ...`. Das Lesen und der eigentliche MCP-HTTP-Aufruf sind getrennte mögliche Freigaben. Der historische Befehl wurde nicht freigegeben; er beweist keinen erfolgten MCP-Abruf. Bei der Handabnahme muss der Nutzer den tatsächlich angezeigten neuen Befehl einschließlich seiner aktuellen Pfade prüfen.

## Latenz des endgültigen Release-Binaries

Nachweis: [evidence-latency.json](../tests/e2e/evidence-latency.json), Lauf `1794333877641`, `target/release/zuruf`. Je ein W/S/D-Einzelwert plus zwei schnelle Captures zur Zuordnungsprüfung. Kein p95, keine Behauptung physikalischer Unerreichbarkeit. Die Debugläufe waren deutlich langsamer und sind nicht die Produktionsmessung.

| Messstrecke / Stufe (ms) | W | S | D |
|---|---:|---:|---:|
| Tastenevent → dauerhaftes Rohbild-Event empfangen | 177,9 | 229,3 | 155,3 |
| Tastenevent → Bestätigungsbeobachtung nach Rendergelegenheit | 205,1 | 248,1 | 187,8 |
| Browser-Vorbereitung inkl. Queue/Overlay-Hide | 29,70 | 2,03 | 35,95 |
| Metadaten-/Maskengeometrie | 1,32 | 0,99 | 4,19 |
| Chromium `Page.captureScreenshot` | 103,51 | 77,51 | 90,24 |
| Frameprüfung, PNG-Decoding/Maskierung | 2,48 | 0,79 | 1,64 |
| Rest bis Rohbild-Event (Differenz; keine einzelne Stufe) | 40,90 | 147,98 | 23,28 |

Der Rest umfasst Eingang/Marker, lokale Persistenz und Eventzustellung; bei S insbesondere die absichtliche Auswahldauer. S misst ab Tastendruck, nicht erst ab Loslassen der Auswahl. Das Bildschirm-PNG dominiert die gemessene Browserarbeit. Unnötige Layoutabfragen für W/D entfallen; Geometrie und Masken kommen aus einem Snapshot, unabhängige CDP-Abfragen überlappen. Markierung erfolgt erst nach Raw-Persistenz.

Zeitquelle: Das Content Script meldet das vertrauenswürdige Keyevent mit `performance.timeOrigin + event.timeStamp`. Raw-Zeit ist der Eingang des ausschließlich Rohassets enthaltenden Annotationevents über den Events-WebSocket nach Asset-fsync und Snapshotcommit: eine obere Beobachtungsgrenze, kein HTTP-Pollwert. Die Bestätigung wird sichtbar gesetzt; nach zwei `requestAnimationFrame`-Callbacks wird der Beobachtungsmarker erfasst, also nach einer Rendergelegenheit. Es ist keine physische Display-/Photonenmessung. Ablaufpolling steuert den Test, liefert aber diese Messwerte nicht. Der Test wartet ausdrücklich auf das Bestätigungsereignis, damit auch D vollständig erfasst wird.

**Das Ziel <150 ms ist mit diesen Messungen nicht nachgewiesen.** Der gewählte Beobachtungsmarker liegt darüber; der erste Release-Durchlauf lag bei W bei etwa 137 ms bis Raw und 162 ms bis Bestätigung, der endgültige Lauf bei 178/205 ms. Desktoplast und Rendering schwanken. Eine Behauptung, das Ziel sei auf dieser Hardware grundsätzlich unmöglich, wäre nicht belegt.

## Endgültige Prüfungen

Alle Builds/Tests/Server/Browser speicherbegrenzt, Cargo mit zwei Jobs. Bestanden: UI-Build; Extension check/lint/build; Rust Release-/Debug-/Laborbuild; `cargo test --workspace -j 2` (108 bestanden, fünf ausdrücklich ignorierte Liveprüfungen); `cargo clippy --workspace --all-targets -j 2 -- -D warnings`; `cargo fmt --all --check`. Der headed Browser wurde zusätzlich ausdrücklich ausgeführt.

`bash tests/e2e/run.sh`: **6 PASS, 0 FAIL** – workspace, mcp-sdk, headed-browser, captures (Release), app, install. Logs: `.spike/acceptance/{workspace,mcp-sdk,headed-browser,captures,app,install,verification}.log`. Live-Follow-up separat bestanden: [evidence-followup.json](../tests/e2e/evidence-followup.json), `FOLLOWUP_OK` am bestehenden Thread; MCP-Agentenabruf weiterhin manuell offen. [evidence-completion.json](../tests/e2e/evidence-completion.json) dokumentiert die echte Parallelität und den anschließenden Freigabe-/Timeoutabbruch, nicht einen vollständig bestandenen Completionlauf.

Der Installationsrundlauf startet das installierte Binary als kurzlebigen Daemon mit Argument `daemon`, öffnet die App aus dem Wegwerfpräfix, prüft PID-Wiederverwendung/Fokus und Doctor und entfernt anschließend Installation/Daten nach ausdrücklicher Test-Purgebestätigung. `doctor`: Chromium 155, Voxtype 1.1.1, T3 0.1.7, Loopbackport und Hyprland-/PipeWire-Sockets vorhanden; deutscher Modellhinweis bleibt. Keine reale Benutzer-Unit aktiviert.

Abschließende Bereinigung: eigene Daemons/Browser/Fixtureprozesse beendet, keine Zuruf-Unit aktiv, keine Zuruf-Fenster, keine Zuruf-Runtimebinds. Beide erstellten T3-Threads ohne aktiven Run; neu erzeugte temporäre Laborprofile und private Wiring-Testzustände entfernt; bestehende App-Smoke-Testverzeichnisse bleiben als Testdaten erhalten. Keine Änderungen an T3, Voxtype oder `~/.config/hypr`; kein Push/PR.


## Erststartpfad – 10.11.2026 (`feat/firstrun`)

Zwei echte Läufe von `tests/e2e/firstrun.mjs` mit leeren temporären Daten-, Zustands- und Konfigurationsverzeichnissen waren erfolgreich. Kein Fakeflag, kein Seed, keine Übernahme installierter Credentials. Der zweite Lauf enthält zusätzlich die CLI-Livepaarung. Geprüfte Folge:

1. `/state`: T3 `disconnected`, keine Version, kein Fehler, keine Projekte.
2. `/t3/pair` ohne Origin erkennt das dokumentierte lokale T3 anhand der OAuth-Metadaten und verbindet sofort mit echter Version `0.1.7`. Wiederholtes Pairing erzeugt keine weitere Identität. `/t3/reconnect` und CLI-Pairing gegen den laufenden Testdaemon funktionieren live.
3. `/t3/catalog` und `/t3/projects` liefern reale Daten. Projektanlage für den registrierten Zuruf-Root löst `t3ProjectId` auf und aktiviert automatisch Review und leeren Draft.
4. Erstes Profil aus dem Livekatalog ergänzt Projektstandard und bereits offenen Draft.
5. Sessionanlage und Start öffnen die lokale Fixture-Basis-URL im eigenen Chromium-Profil. Review an und ein W-Keyevent erzeugen persistiertes markiertes Bild.
6. Dispatch erzeugt genau einen echten Thread mit `zuruf-test-firstrun-`-Titel und ausdrücklichem Dateiänderungsverbot. Sofortiger Interrupt; beide Threads anschließend über die externe Thread-Leseschnittstelle als `interrupted`, ohne aktiven Run bestätigt. Keine Providerfertigstellung oder menschliche Approvalabnahme behauptet.
7. Zweites Projekt schaltet Review aus; Wechsel zurück stellt Projekt-Draft und zuletzt verwendete Sitzung wieder her. Wechsel zur zweiten Sitzungsauswahl bleibt leer. Browserstop und Daemonshutdown erfolgreich.

Behoben: eingefrorener Origin im realen Adapter, CLI-Pairing ohne Livewirkung, fehlende HTTP-Paarung, Fakeversion und Fehlerzustand vor Erstpaarung, unvollständige Projektaktivierung, fehlende erste Profilzuweisung, unzulässiger leerer Reviewstart und pauschale Löschblockade durch vorhandene Reviews. Zusätzlich werden Origins und Credentials beim Reconnect neu gelesen, verlorene Credentialdateien nicht aus dem Adaptercache ersetzt, unbekannte Versionen gelöscht und fehlende Credentials beim normalen Start nicht automatisch neu gepaart. Projektpfade werden kanonisiert und auf Duplikate geprüft; Basis-URLs müssen HTTP/HTTPS verwenden. Zulässiges Projektlöschen entfernt zugehörige Snapshot-Einträge, erhält Dateien und Browserprofile auf Platte. Laufende Sitzungen und nichtterminale Tasks verhindern das Löschen.

Fake-HTTP-Regressionen prüfen ungültige Pfade/URLs, Duplikate, erste Profildefaults, Review ohne Sitzung, Projektaktivierung und Rückwechsel, Löschverweigerung bei laufender Sitzung sowie laufendem Task und aktive Auswahl nach Löschung. Workspace-Tests, Clippy mit `-D warnings`, Formatprüfung, Extension check/lint/build und unveränderter UI-Build bestehen unter Speicherlimits (Cargo mit zwei Jobs).

Erzeugte Testartefakte in T3:

- <t3-thread://v2/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp:launch:3ca4a760501b343568b170e525ddd2370abc99f05d50ac29303688a2735874b3>
- <t3-thread://v2/faa29a94-47bb-4c46-99e4-5a28099b0307/mcp:launch:9c4799b1287b1afce71839cdc55275cc8ab239cdbd306d5d3f926c5df7b42e1b>
- Zusätzliche OAuth-Sessions `c98935fa-6bd4-4135-b1a3-76b09b4f83ab` und `3ae8e365-edec-47e1-bde1-7578e28403d4`: beide erfolgreich über `t3 auth session revoke` widerrufen. Keine Tokens protokolliert.

Alle Testverzeichnisse wurden entfernt. Nachprüfung: keine Zuruf-Runtimebindungen, keine Zuruf-Units, keine Testdaemon-/Browserprozesse oder Testfenster. Installiertes `zuruf.service` unverändert inaktiv (`MainPID=0`, kein Startzeitstempel); installierte Dateien und Daten wurden nicht verwendet oder verändert. Keine Änderungen an T3, Voxtype, Hyprland-Konfiguration, UI-Quellcode oder Overlay-CSS. Kein Push, kein PR.

Offen bleibt die parallele Setup-/Projektoberfläche des Projektleiters; dieser Nachweis betrifft den öffentlichen API-Pfad. Sprache/Modelleinrichtung, Installation selbst und andere bestehende Abnahmegrenzen dieses Dokuments werden durch diesen isolierten Erststartlauf nicht erneut abgenommen.

## Desktop-Nachprüfung des Updates – 11.11.2026

Grundlage ist GitHub-Stand `4b52425` mit lokalen Korrekturen für getrennte Teststarts und den Fensterschließtest unter Hyprland 0.56.2. `ZURUF_INSTALL_TEST=1` startet einen eigenen Hintergrunddienst mit den Testpfaden; die installierte Benutzer-Unit bleibt dabei unberührt. Der Hyprland-Schließtest verwendet die beobachtete Adresse seines eigenen Fensters und unterstützt Lua- sowie Legacy-Dispatcher. Bereits geöffnete Zuruf-Fenster werden erhalten.

- UI-, Seiten-Agent-, Debug- und Release-Build erfolgreich; 121 reguläre Rust-Tests bestanden, fünf Liveprüfungen im Standardlauf ignoriert. Die ignorierte Browser-Sitzungsprüfung wurde zusätzlich erfolgreich ausgeführt. MCP-SDK, Installationsrundlauf, Clippy mit `-D warnings` und Rust-Formatprüfung bestehen. Die TypeScript-Prüfung und sieben Seiten-Hotkeytests bestehen; Prettier meldet vorhandene Formatabweichungen in `apps/page/src/agent.ts` und `protocol.ts`.
- Browser- und App-Durchstich unter echtem Wayland mit `--disable-gpu`: W/S/D, Auswahlabbruch, unterdrückter Auswahlklick, unveränderte Texteingabe, Seitenmeldungen, feste Seitenbreite, Playwright-Steuerung und -Tabs, geschütztes App-Panel, Takeover, Stopp sowie Erkennung und Neustart eines abgestürzten Fensters bestanden.
- Derselbe App-Durchstich wurde nach der Installation mit den tatsächlich installierten Release-Programmen und der installierten Chromium-Laufzeit bestanden, während das normale Zuruf-App-Fenster offen blieb. Die gespeicherten Entitäten blieben beim Update erhalten; die vorhandene Testsitzung ließ sich mit aktivem Seiten-Agenten starten und stoppen. Die bestehende T3-Verbindung und der Livekatalog funktionieren. Kein echter Agentenauftrag versendet und kein Mikrofon eingeschaltet.
- Im normalen GPU-Pfad hingen wiederholte Aufnahmen in der vorgefundenen Desktopumgebung; Software-Rendering bestand. Für diese lokale Installation ist `ZURUF_SHELL_ARGS=--disable-gpu` als systemd-Benutzer-Override gesetzt. Die Chromium-Sandbox bleibt aktiviert.
- Die eigene Fokusprüfung konnte am schwarzen, nicht interaktiven Desktop nicht bestätigt werden. Der Nutzer war nicht am Rechner. Persönliche Sicht-/Fokusprüfung, gesprochenes Diktat und ein echter Versand-/Agentendurchlauf bleiben offen; die bisherigen manuellen Abnahmegrenzen gelten weiter.

Prüflogs liegen lokal unter `.spike/acceptance/update-*.log`. Die vorherige Installation und die Laufzeitdaten sind privat außerhalb des Git-Checkouts gesichert.
