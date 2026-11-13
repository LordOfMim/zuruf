# Zuruf

**Webseiten prüfen. Feedback festhalten. Coding-Agenten gezielt weiterarbeiten lassen.**

[![Build und Tests](https://github.com/LordOfMim/zuruf/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/LordOfMim/zuruf/actions/workflows/build.yml)
[![Downloads](https://img.shields.io/github/v/release/LordOfMim/zuruf)](https://github.com/LordOfMim/zuruf/releases/latest)
[![Lizenz: MIT](https://img.shields.io/badge/Lizenz-MIT-blue.svg)](LICENSE)

Zuruf ist eine lokale Desktop-App für Browser-Reviews: Links sitzt das Review-Werkzeug, rechts die geprüfte Website in einem integrierten Chromium-Browser. Du markierst einen Fehler, sprichst oder schreibst einen Kommentar dazu und übergibst die gesammelten Hinweise als Aufgabe an einen Coding-Agenten in [T3 Code](https://app.t3.codes).

Rust für App und Browser-Shell, Solid für die Oberfläche. Keine Browser-Extension, kein Electron und kein Node zur Laufzeit.

![Zuruf: Review-Werkzeug neben der geprüften Seite](docs/bilder/fenster.png)

*Die Screenshots zeigen Beispieldaten.*

## Herunterladen

Die [aktuelle Veröffentlichung](https://github.com/LordOfMim/zuruf/releases/latest) enthält fertige Programme mit Chromium. Du brauchst zum Installieren weder Rust noch Node.

| System | Download | Installation |
| --- | --- | --- |
| Windows 10/11, x64 | [Windows-Installer](https://github.com/LordOfMim/zuruf/releases/latest/download/Zuruf-Setup-windows-x64.exe) | Starten und dem Assistenten folgen. Keine Administratorrechte erforderlich. |
| Windows, portabel | [ZIP-Paket](https://github.com/LordOfMim/zuruf/releases/latest/download/zuruf-windows-x64.zip) | Entpacken und `Zuruf.exe` öffnen. |
| Ubuntu / Debian, x64 | [Debian-Paket](https://github.com/LordOfMim/zuruf/releases/latest/download/zuruf-linux-amd64.deb) | Über die Paketverwaltung öffnen oder `sudo apt install ./zuruf-linux-amd64.deb`. |
| Arch / andere Linux-Distributionen, x64 | [Linux-Bundle](https://github.com/LordOfMim/zuruf/releases/latest/download/zuruf-linux-x64.tar.gz) | Entpacken und `./install.sh` ausführen. Installiert in deinen Benutzerbereich. |
| macOS 12 oder neuer, Apple silicon | [Disk-Image](https://github.com/LordOfMim/zuruf/releases/latest/download/Zuruf-macos-arm64.dmg) | Öffnen und Zuruf in den Programme-Ordner ziehen. Beim ersten Start siehe unten. |
| macOS, ohne Image | [ZIP-Paket](https://github.com/LordOfMim/zuruf/releases/latest/download/zuruf-macos-arm64.zip) | Entpacken und `Zuruf.app` nach „Programme“ verschieben. |

Linux benötigt glibc ab 2.35 sowie die üblichen GTK-/NSS-/Audio-Bibliotheken von Chromium. Das Debian-Paket bringt die passenden Paketabhängigkeiten mit. Unter Arch: `gtk3 nss nspr alsa-lib libxss`. X11 und Wayland werden unterstützt. Linux und Windows auf ARM sowie Intel-Macs sind derzeit keine Release-Ziele.

**macOS, erster Start:** Zuruf ist nur ad hoc signiert, nicht von Apple notarisiert. macOS blockiert deshalb den ersten Start. Einmal unter *Systemeinstellungen › Datenschutz & Sicherheit* „Dennoch öffnen“ wählen oder im Terminal `xattr -dr com.apple.quarantine /Applications/Zuruf.app` ausführen. Die CLI erreichst du mit `ln -s /Applications/Zuruf.app/Contents/MacOS/zuruf /usr/local/bin/zuruf`. Starte Zuruf über die App, damit die Mikrofonfreigabe für das Diktat bei Zuruf liegt; nach Updates fragt macOS unter Umständen erneut.

Die Release-Seite enthält SHA-256-Prüfsummen und Lizenzhinweise. Der Windows-Installer ist derzeit nicht Authenticode-signiert, die macOS-App nicht notarisiert. Veröffentlichungen werden aus dem zugehörigen Git-Tag auf GitHub gebaut; Details stehen unter [Releases und Builds](docs/releases.md).

## Was Zuruf kann

- **Feedback dort festhalten, wo es entsteht.** Ganze Ansichten aufnehmen, ein Element auswählen oder einen Bereich aufziehen. Screenshot, Kommentar, URL und Elementkontext bleiben zusammen.
- **Kontext statt Rätsel.** Konsolenfehler, unbehandelte Ausnahmen und fehlgeschlagene Anfragen der Seite gehen mit der Aufnahme an den Agenten.
- **Echte Seitenbreiten.** Vollbild, 1024, 768 oder 390 Pixel: Die Seite bekommt einen echten Viewport in dieser Breite.
- **Mit dem Agenten zusammenarbeiten.** Im Browser sind sein Zeiger, Fortschrittsmeldungen und Rückfragen sichtbar. Du kannst die Steuerung jederzeit übernehmen.
- **Aufgaben übersichtlich halten.** Auch abgebrochene oder fehlgeschlagene Aufgaben lassen sich abschließen und später wieder öffnen. Der ursprüngliche Status bleibt erhalten.
- **Nach Unterbrechungen weiterarbeiten.** Geschlossene Fenster, Prozessabbrüche und Hänger lösen die Wiederherstellung der aktiven Browser-Sitzung aus. Gespeicherte Tab-Adressen und Profildaten bleiben erhalten.
- **Eigene Tastenkürzel.** Die Belegung lässt sich in der App ändern. Desktopweite Hotkeys stehen unter Hyprland zur Verfügung.

| Element auswählen | Bereich aufziehen | Mit einem Agenten arbeiten |
| --- | --- | --- |
| ![Ausgewähltes Element](docs/bilder/element-waehlen.png) | ![Aufgezogener Bereich](docs/bilder/bereich-aufziehen.png) | ![Agenten-Zeiger und Rückfrage](docs/bilder/agent.png) |

## Erste Schritte

1. **T3 Code starten und verbinden.** In Zuruf auf „Verbinden“ klicken und die Paarung in T3 bestätigen. T3 und die dort eingerichteten Modellanbieter werden separat benötigt.
2. **Projekt hinzufügen.** Den vorhandenen Repository-Ordner und die URL deiner laufenden Webanwendung auswählen, etwa `http://localhost:5173`.
3. **Agentenprofil wählen.** Anbieter und Modell kommen aus dem Katalog deiner T3-Installation.
4. **Sitzung starten und Review einschalten.** Die Website öffnet sich direkt neben dem Review-Werkzeug.
5. **Aufnehmen, kommentieren, senden.** Aufnahmen werden als Aufgabe an den gewählten Agenten übergeben. Freigaben erteilst du weiterhin in T3.

T3 wird nicht mitinstalliert. Browser und lokale App lassen sich vorher öffnen; Aufgabenversand setzt die Verbindung zu T3 und einen eingerichteten Anbieter voraus.

## Tastenkürzel

Im Review-Modus außerhalb von Textfeldern:

| Taste | Aktion |
| --- | --- |
| `W` | Ganze Ansicht aufnehmen |
| `D` | Element auswählen; Klick oder Enter bestätigt |
| `S` | Bereich aufziehen |
| `E` | Kommentar schreiben |
| `R` | Diktat ein/aus, wenn verfügbar |
| `Umschalt+N` | Gesammelte Einträge senden |
| `Esc` | Auswahl abbrechen |

In Textfeldern bleibt das Tippen unbeeinflusst. Mit `Alt` dazu, etwa `Alt+W`, kannst du auch dort eine Aufnahme auslösen. Die Belegung ist einstellbar.

Der Browser kennt `Strg+T`, `Strg+W`, `Strg+L`, `Strg+R`, `Alt+←/→`, Zoom und `F12`. Unter macOS gilt `⌘` statt `Strg`, Zurück und Vorwärts sind `⌘[` und `⌘]`.

## Plattformen und Grenzen

| Funktion | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Integrierter Browser, Screenshots, Kommentare | Ja | Ja | Ja |
| T3-Anbindung, Aufgaben, Browsersteuerung durch Agenten | Ja | Ja | Ja |
| Wiederherstellung nach Browser-Unterbrechungen | Ja | Ja | Ja |
| Diktat | Voxtype, whisper.cpp oder Online-Dienst | whisper.cpp oder Online-Dienst | Derzeit nicht verfügbar |
| Desktopweite Tastenkürzel | Hyprland | Derzeit nicht verfügbar | Derzeit nicht verfügbar |
| Tastenkürzel in der geprüften Seite | Ja | Ja | Ja |

Für das Diktat wählst du unter *Einstellungen › Diktat* einen Dienst: lokal mit Voxtype (nur Linux) oder whisper.cpp (`whisper-cli` und eine ggml-Modelldatei), oder online über OpenAI, Groq, Mistral, Deepgram, ElevenLabs oder einen eigenen OpenAI-kompatiblen Server. Für Online-Dienste brauchst du einen eigenen API-Schlüssel; er wird nur lokal gespeichert und nie wieder angezeigt.

Der Browser ist für Reviews gedacht. Downloads, Passwortverwaltung und Kamera-/Mikrofon-Berechtigungsdialoge einer Website gehören derzeit nicht zum Umfang. Wiederhergestellt werden gespeicherte Daten und Tab-Adressen; ungespeicherte Formulareingaben gehören nicht dazu. Die Oberfläche ist deutsch.

## Daten und Datenschutz

Projekte, Screenshots, Aufgaben und Browserprofile liegen lokal. Jede Sitzung verwendet ein eigenes Profil. Agenten erhalten einen widerrufbaren Zugang zu ihrer Aufgabe und nur bei Freigabe Browserkontrolle. Menschliches Übernehmen hat Vorrang.

Beim **Senden** geht dein Feedback über T3 an den dort gewählten Anbieter. Diktat geht an den Dienst, den du dafür wählst: Mit einem Online-Dienst verlässt die Tonaufnahme den Rechner, mit whisper.cpp nicht; Voxtype transkribiert je nach deiner bestehenden Konfiguration lokal oder über einen Remote-Endpunkt. Stille wird nie verschickt. „Lokal“ bedeutet daher keine pauschale Zusage, dass übergebene Inhalte den Rechner nie verlassen.

| System | Persönliche Daten |
| --- | --- |
| Linux | `~/.local/share/zuruf`, `~/.local/state/zuruf`, `~/.config/zuruf` beziehungsweise deine XDG-Pfade |
| macOS | `~/Library/Application Support/Zuruf/data`, `state` und `config` |
| Windows | `%LOCALAPPDATA%\Zuruf\data`, `state` und `config` |

Updates und die normalen Deinstallationsprogramme erhalten diese Daten. Tokens und Zugangsdaten gehören weder in Issues noch in Screenshots. [Sicherheitsprobleme melden](SECURITY.md).

## Aus dem Quellcode bauen

Für die normale Nutzung sind die Downloads oben vorgesehen. Zum Entwickeln werden Rust 1.99.1, Node 26, pnpm 11, Python 3.12, CMake und Ninja benötigt; unter Windows zusätzlich die Visual Studio C++ Build Tools und ein Windows SDK.

```sh
git clone https://github.com/LordOfMim/zuruf.git
cd zuruf
pnpm install --frozen-lockfile
pnpm --filter @zuruf/ui build
pnpm --filter @zuruf/page build
cargo build --release --workspace --locked
```

Cargo lädt die festgelegte Chromium-Laufzeit bei Bedarf. Der Build braucht mehrere GB freien Speicherplatz und rund 4 GB Arbeitsspeicher. Unter Linux installiert `./install.sh` einen selbst gebauten Stand. Für Windows verwendet der Browser den offiziellen CEF-Bootstrap mit einer Rust-DLL; deshalb zum Starten das [Release-Bundle erstellen](docs/releases.md), nicht die ungebündelte Shell-EXE verwenden.

[Mitwirken](CONTRIBUTING.md) · [Benutzerhandbuch](docs/benutzerhandbuch.md) · [Architektur](docs/architecture.md) · [Builds und Veröffentlichungen](docs/releases.md)

## Lizenz

Zuruf steht unter der [MIT-Lizenz](LICENSE). Chromium, CEF, Rust-Abhängigkeiten, Solid und Schriftarten behalten ihre jeweiligen Lizenzen; die fertigen Pakete enthalten die zugehörigen Lizenztexte und Hinweise.
