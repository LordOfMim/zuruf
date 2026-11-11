# Zuruf – Benutzerhandbuch

## Installation und Start

Voraussetzungen: Linux mit Hyprland/Wayland, PipeWire, Voxtype und T3 Code. Einen Browser bringt Zuruf selbst mit; System-Chromium und eine Extension werden nicht mehr gebraucht. Builds benötigen Rust stable über mise, Node 26, pnpm 11 sowie `cmake` und `ninja`. `./install.sh` baut UI, Seiten-Agent und Rust in dieser Reihenfolge unter 4 GiB Speichergrenze mit vier Cargo-Jobs; beim ersten Mal lädt der Build die Chromium-Laufzeit (rund 140 MB) nach `~/.cache/zuruf/cef`. `--skip-build` verwendet fertige Release-Artefakte. Installiert werden `zuruf`, das Browserfenster samt Chromium-Laufzeit unter `~/.local/lib/zuruf` (rund 330 MB), Benutzer-Unit, Starter und Zuruf-Icon. Die Installation aktiviert keinen Dienst.

```sh
zuruf doctor
systemctl --user daemon-reload
systemctl --user enable --now zuruf.service
zuruf open --view sessions
```

`open` startet bei Bedarf den Daemon und zeigt die App: im Fenster der laufenden Sitzung, sonst in einem eigenen Fenster mit privatem Profil. Wiederholtes `open --view tasks|sessions|settings|review` fordert den Viewwechsel an und holt das vorhandene Fenster nach vorn. Alle Zuruf-Fenster tragen die Fensterkennung (App-ID) `zuruf`; damit lassen sie sich in Hyprland-Regeln ansprechen, und der Starter findet sein Icon.

`PREFIX`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME` und die spezifischen `ZURUF_*_DIR` ändern Ziele. Für CLI und Dienst müssen dieselben Pfade gelten. `.env.example` ist Dokumentation und wird nicht automatisch geladen. Tests installieren ausschließlich in einen Wegwerfpräfix.

## Erster Start

Ein leeres Datenverzeichnis enthält absichtlich keine Demo-Projekte oder Fakeversion. Die T3-Verbindung meldet zunächst `disconnected` ohne Fehlermeldung. Der Einrichtungsclient verbindet über `POST /api/v1/t3/pair` mit `{origin?}`; ohne Adresse verwendet Zuruf den gespeicherten Origin oder prüft die dokumentierte lokale Adresse `http://127.0.0.1:3773` anhand ihrer OAuth-Metadaten. Antwortet dort kein T3, muss die lokale Adresse angegeben werden. Die Verbindung gilt sofort, ein Dienstneustart ist nicht erforderlich. `POST /api/v1/t3/reconnect` liest gespeicherte Adresse und Credentials erneut. Gültige Credentials werden wiederverwendet.

Danach Projekt mit vorhandenem Repository-Verzeichnis und HTTP-/HTTPS-Basis-URL anlegen. Derselbe Repository-Pfad darf nur einmal registriert werden. Zuruf ordnet bei erreichbarem T3 dessen Projekt anhand des Workspace-Roots zu und erzeugt automatisch Review und leeren Draft. Ein Profil aus dem Livekatalog wählen; das erste angelegte Profil wird auch bereits vorhandenen Projekten und offenen Drafts ohne Profil zugeordnet. Eine isolierte Browser-Sitzung für das Projekt anlegen und starten: Zurufs Browserfenster öffnet die Basis-URL im eigenen Sitzungsprofil, die App sitzt links daneben. Anschließend Review einschalten und mit W aufnehmen. Ohne Projekt oder laufende aktive Sitzung meldet Review eine verständliche Fehlermeldung.

Projektwechsel schaltet Review aus und aktiviert den projektspezifischen Draft sowie die zuletzt verwendete Sitzung oder keine Sitzung. Löschen wird bei laufenden Sitzungen oder nicht abgeschlossenen Aufgaben abgewiesen. Beim zulässigen Löschen verschwinden Projekt und zugehörige Einträge aus dem Snapshot; Capturedateien und Browserprofil-Verzeichnisse bleiben konservativ auf der Platte erhalten.

Diese API-Schritte sind implementiert und live geprüft; die parallel entwickelte Setup-Oberfläche wird hiermit nicht als abgenommen erklärt.

## T3 verbinden

Den tatsächlichen lokalen T3-Origin feststellen, dann `zuruf t3 pair --origin http://127.0.0.1:<port>` ausführen. Bei laufendem Daemon verwendet die CLI die Pairing-Route und verbindet live; ohne Daemon speichert sie die Paarung für den nächsten Start. Zuruf nutzt die verifizierte externe Pairing-/MCP-Schnittstelle. OAuth-Credentials liegen privat im Zustandsverzeichnis. T3-Dateien und Datenbanken werden nicht verändert. Ein abgelaufener Zugang verlangt ausdrückliches erneutes Pairing. Der ausdrücklich gewählte CLI-Fallback (`ZURUF_T3_TRANSPORT=cli`) unterstützt weniger Funktionen, insbesondere keine Bildattachments und keinen Livekatalog.

Provider, Modell, Optionen und Arbeitsverzeichnis werden pro Batch eingefroren. Für parallele Schreibaufträge separate Worktrees wählen. `unknown_remote_state` verlangt Abgleich und manuelle Auflösung; ein Retry erzeugt dabei keinen stillen zweiten Thread. Höchstens acht Bilder können pro T3-Nachricht versendet werden.

## Review und Sprache

Projekt, Review und Session auswählen, im verwalteten Browser nötigenfalls manuell anmelden und Review aktivieren. `W` erfasst den Viewport, `S` einen gezogenen Ausschnitt, `D` das Element unter dem Cursor. In Textfeldern, Editoren und Contenteditable bleiben einzelne Tasten normale Eingaben; dieselbe Taste mit Alt (z. B. Alt+R) löst die Aktion auch dort aus. Kürzel mit Strg, Alt oder Super gelten überall. Zuruf fängt seine Tasten vor der Seite ab, einschließlich Wiederholung und Loslassen. `Esc` bricht eine Auswahl ab. `E` öffnet den Textkommentar, `R` steuert Sprache. Die Bindings sind in den Einstellungen konfigurierbar. Globale Bindings benötigen Modifier und werden nur zur Laufzeit gesetzt.

Nach dauerhafter Rohbildpersistenz bestätigt die Seite „Gespeichert“. Die markierte Kopie entsteht anschließend; Dispatch wartet auf beide Bilder. Ein Absturz vor Abschluss hinterlässt eine sichtbar als unterbrochen kommentierte Annotation und blockiert deren unvollständigen Versand. Neu aufnehmen beziehungsweise den Eintrag prüfen und entfernen; automatische Rekonstruktion einer verlorenen Captureoperation gibt es nicht.

Continuous-Aufnahme setzt PCM-Grenzen an Capture und `Shift+N`; Finalisierung läuft im Hintergrund. Der Daemon-Fallback hat eine Stop-/Start-Lücke. `Shift+N` versiegelt den alten Batch und öffnet sofort den neuen Draft. Späte Transkripte bleiben am alten Batch. Vor Versand Transkripte und Bilder prüfen. Ohne Modell in den Einstellungen gilt die eigene Voxtype-Konfiguration, auch ein Remote-Endpunkt mit API-Schlüssel aus den Environment-Dateien von `voxtype.service`; dann verlässt die Tonaufnahme den Rechner wie bei Voxtype selbst. Ein in Zuruf eingetragenes Modell erzwingt lokale Erkennung und muss vorhanden sein. Deutsche Sprachqualität bleibt manuell offen. Zuruf lädt keine Modelle und verändert oder startet den vorhandenen Voxtype-Daemon nicht neu.

## Was die Seite selbst meldet

Zu jeder Aufnahme merkt sich Zuruf, was die Seite bis dahin von sich aus gemeldet hat: Konsolenfehler und -warnungen, unbehandelte Ausnahmen und fehlgeschlagene Anfragen (etwa ein 404 oder 500), jeweils mit Quelle und Zeile. Am Eintrag steht dann zum Beispiel „2 Fehler“; ein Klick klappt die Liste auf. Die Meldungen gehen im Manifest (`diagnostics` je Annotation) mit an den Agenten, der damit oft schon die Ursache in der Hand hat.

Gesammelt wird je Tab seit dem Laden des aktuellen Dokuments, höchstens die letzten 20 Meldungen; gleiche Meldungen hintereinander werden gezählt statt wiederholt. Adressen durchlaufen dieselbe Schwärzung wie alle anderen. Meldungstexte werden nicht inhaltlich geprüft – wer Geheimnisse in die Konsole schreibt, sollte die Liste vor dem Senden ansehen.

## Aufgaben und Agentenzugriff

Das Board zeigt parallele Tasks, Status, Antworten und Freigabeanforderungen. Follow-ups können nach Ende eines Runs an denselben Thread gesendet werden. **T3-Freigaben erteilt ausschließlich der Nutzer in T3.** Zuruf gibt sie nicht stellvertretend frei.

Für Captures einer einzelnen Session erhält jeder Task einen eigenen MCP-Zugang. Die externe T3-Launchschnittstelle hat keinen privaten MCP-/Environment-Parameter. Deshalb nennt der Prompt nur den Pfad einer privaten Datei unter `state/agents/<task>/credential.json` (0600). Der Agent liest die Datei lokal, ohne ihren Inhalt auszugeben, und verwendet URL, Origin und Bearer für MCP-JSON-RPC. Das ist eine Dateisystemübergabe, keine automatisch registrierte MCP-Verbindung im Provider. Prozesse desselben Unix-Nutzers können diese Datei lesen; eine OS-Sandbox wird nicht behauptet. Gemischte Mehrsessionbatches werden abgewiesen.

MCP bietet Manifest, Annotationen, Bilder und Fortschrittsmeldungen. CDP verlangt zusätzlich eine aktive Kontrolllease; Erneuerung ist atomar und kann weder Ablauf noch menschliches Takeover überholen. Leaseablauf aktualisiert die Anzeige selbstständig. Abschluss, Abbruch und Interrupt widerrufen den Taskzugang; Shutdown entfernt die Dateien, Neustart verwirft alte Credentials. Laufende Tasks werden anhand ihrer gespeicherten Receipts weiterverfolgt. Nach Neustart muss ein Agentenzugang neu bereitgestellt werden; automatische Wiederanmeldung eines bereits laufenden Agenten bleibt offen.

Menschliches Takeover sperrt neue Proxybefehle sofort. Bereits ausgeführtes JavaScript kann weiterwirken; Browserkontrolle ist kein Rollback. Der rohe CDP-Port des Browserfensters ist nur auf Loopback erreichbar, aber keine Sicherheitsgrenze gegenüber lokalen Prozessen.

## Datenschutz

URLs verlieren Zugangsdaten, Fragmente und sensible Queryparameter; DOM-Attribute sind begrenzt, Eingabewerte werden nicht gespeichert. Screenshots maskieren Passwortfelder und `[data-zuruf-private]` im Hauptdokument vor der Persistenz. `ZURUF_MASK_SELECTORS` ergänzt CSS-Selektoren (kommagetrennt). Ungültige Selektoren lassen den Capture fehlschlagen. Überlappende DOM-Metadaten werden verworfen.

Automatische semantische Erkennung beliebiger personenbezogener Texte/Bilder, geschlossene Shadow Roots und fremde Frames sind damit nicht abgedeckt. Masken ersetzen keine Sichtprüfung und ausdrückliche Freigabe vor Weitergabe an einen externen Provider. Kein Secret gehört in Notizen, URLs, Prompts oder Logs.

## Skill für Agenten

`skills/zuruf/SKILL.md` beschreibt CLI, Sitzungen und Task-MCP für Agenten. Einbinden per Symlink des Ordners in das Skill-Verzeichnis des jeweiligen Agenten.

## Deinstallation und Diagnose

`./uninstall.sh` entfernt Programm, Browserfenster samt Chromium-Laufzeit, Unit, Desktopdatei und Icon; Benutzerdaten bleiben. `--purge` löscht sie erst nach Eingabe von `LÖSCHEN`. Dieselben Zielvariablen wie bei der Installation verwenden.

`zuruf doctor` prüft lokale Programme, Sockets, T3-Port und Modelle; das bestätigt noch keine Anmeldung oder deutsche Diktatqualität. `zuruf stop` beendet Dienst und alle Zuruf-Fenster. Keine Hyprland-Konfigurationsdatei wird geändert. Abnahme und offene Handprüfungen stehen in [abnahme.md](abnahme.md), der reproduzierbare Sammellauf unter `tests/e2e/run.sh`.

## Das Browserfenster

Zuruf hat einen eigenen Browser: Chromium ist eingebaut, es gibt keine Extension und kein zweites Programm, zwischen dem man wechseln müsste. Ein Sitzungsfenster zeigt links Zuruf und rechts die geprüfte Seite mit Tab-Leiste und Adressleiste.

| Taste | Wirkung |
|---|---|
| Strg+T, Strg+W | Tab öffnen, Tab schließen |
| Strg+Tab, Strg+Umschalt+Tab | nächster, vorheriger Tab |
| Strg+L | Adresse eingeben |
| Strg+R oder F5, Strg+Umschalt+R | neu laden, ohne Zwischenspeicher neu laden |
| Alt+←, Alt+→ | zurück, vorwärts |
| Strg+Plus, Strg+Minus, Strg+0 | Zoom |
| F12 | Entwicklerwerkzeuge in eigenem Fenster |

Jeder Tab hat sein eigenes Schließen-Kreuz; während eine Seite lädt, bricht der Neu-laden-Knopf das Laden ab.

**Seitenbreite.** Der Knopf rechts neben der Adressleiste schaltet die Breite um, in der die Seite gezeigt wird: volle Breite, 1024, 768 und 390 Pixel. Die Seite sitzt dann mittig im Fenster und hat wirklich diese Breite – Media Queries greifen wie auf einem so breiten Bildschirm, und jede Aufnahme hält die Breite fest. Von außen: `POST /api/v1/sessions/<id>/page-width` mit `{"width": 390}` oder `{"width": null}`.

In die Adressleiste getippte Adressen ohne Schema werden für `localhost` und IP-Adressen mit `http://`, sonst mit `https://` geladen. Links, die ein neues Fenster öffnen wollen, bekommen einen Tab. `panel.toggle` (Standard Alt+P) blendet Zuruf aus und ein; die Seite bekommt dann die volle Breite. Fensterfarben folgen dem Thema aus den Einstellungen.

Das Fenster ist ein Review-Browser, kein Alltagsbrowser: Downloads, Berechtigungsabfragen (Kamera, Mikrofon, Standort) und eine Passwortverwaltung gibt es nicht. Anmeldungen bleiben im Profil der Sitzung erhalten.

Wer das Sitzungsfenster schließt, beendet die Sitzung; sie lässt sich unter „Sitzungen“ oder mit `zuruf open` jederzeit wieder starten. Stürzt der Browser ab, zeigt Zuruf die Sitzung als abgestürzt an – ein erneuter Start genügt.

## Ein Fenster oder zwei

Unter „Sitzungen“ legt der Schalter „Ein Fenster / Getrennt“ fest, wo Zuruf lebt. „Ein Fenster“ ist der Standard: Zuruf sitzt mit allen Bereichen links im Sitzungsfenster, die Seite füllt den Rest; das eigene App-Fenster schließt sich beim Start der Sitzung, `zuruf open` holt das Sitzungsfenster nach vorn. Ohne laufende Sitzung öffnet sich das App-Fenster. „Getrennt“: Im Sitzungsfenster sitzt nur das Review-Werkzeug, die übrige App behält ihr eigenes Fenster. Die Wahl gilt ab dem nächsten Start einer Sitzung.
