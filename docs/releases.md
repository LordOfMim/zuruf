# Builds und Veröffentlichungen

## Was veröffentlicht wird

Ein Tag `v<Version aus Cargo.toml>` startet die Release-Pipeline. Linux, Windows und macOS werden aus diesem Tag gebaut, getestet und paketiert. Erst wenn alle drei Jobs erfolgreich sind und ihre Prüfsummen stimmen, wird der GitHub-Release mit allen sechs Downloads veröffentlicht.

| Datei | Inhalt |
| --- | --- |
| `Zuruf-Setup-windows-x64.exe` | Installation für den aktuellen Windows-Benutzer, Startmenü und Deinstallation |
| `zuruf-windows-x64.zip` | Portables Windows-Bundle; Einstieg über `Zuruf.exe` |
| `zuruf-linux-amd64.deb` | Systempaket mit deklarierten Chromium-Abhängigkeiten |
| `zuruf-linux-x64.tar.gz` | Fertige Programme und Benutzer-Installer für Arch und andere Distributionen |
| `Zuruf-macos-arm64.dmg` | Disk-Image mit `Zuruf.app` für Apple silicon (macOS 12+) |
| `zuruf-macos-arm64.zip` | Dieselbe App als ZIP |
| `SHA256SUMS.txt` | SHA-256-Prüfsummen aller Downloads |

Jedes Bundle enthält `BUILD.json` mit Quellcommit, Version und CEF-Archiv sowie Lizenzhinweise unter `share/doc/zuruf` (macOS: `Contents/Resources`). Zusätzlich erstellt GitHub Build-Nachweise für die Pakete. Der Windows-Installer hat noch keine Authenticode-Signatur, die macOS-App nur eine Ad-hoc-Signatur ohne Notarisierung; beides erfordert gesonderte Zertifikate beziehungsweise ein Apple-Entwicklerkonto.

## macOS-App

`Zuruf.app` wird auf einem Apple-silicon-Runner (`macos-15`) gebaut. Das Bundle enthält `Contents/MacOS/{zuruf-launcher,zuruf,zuruf-shell}`, das Chromium-Framework und fünf Helper-Bundles (`zuruf-shell Helper.app`, `… Helper (GPU|Renderer|Plugin|Alerts).app`), die alle dasselbe Programm `zuruf-shell-helper` enthalten. Chromium leitet ihre Namen aus dem Namen der Shell ab. Die Helfer laufen in Chromiums Sandbox. `package.py` signiert von innen nach außen ad hoc (`codesign --sign -`), prüft die Signatur und erstellt Disk-Image (`hdiutil`) und ZIP (`ditto`). `Info.plist` beschreibt die Mikrofonnutzung fürs Diktat. Ohne Notarisierung muss der erste Start einmal bestätigt werden (siehe README).

## Windows-Browser

Seit CEF 138 stellt ein von Chromium gebauter Bootstrap den Windows-Sandbox-Broker bereit. Zuruf folgt dem Modell der offiziellen `cef-rs`-Beispiele: `bootstrap.exe` wird als `zuruf-shell.exe` ausgeliefert und lädt `zuruf-shell.dll`, die `RunWinMain` aus der Rust-Shell exportiert. Der Sandbox-Zeiger wird an CEF durchgereicht. Eine ungebündelte `cargo build`-Shell-EXE ist deshalb kein Windows-Startprogramm.

Der Installer und das portable Root-Startprogramm rufen den kleinen Rust-Launcher auf. Der startet den lokalen Dienst ohne sichtbares Konsolenfenster. Die CLI bleibt separat unter `bin/zuruf.exe` nutzbar. Persönliche Daten liegen unabhängig vom Programmverzeichnis unter `%LOCALAPPDATA%\Zuruf` und bleiben beim Deinstallieren erhalten.

## Lokal paketieren

Rust 1.99.1, Node 26, pnpm 11, Python ab 3.11, CMake/Ninja und die plattformspezifischen Build-Werkzeuge installieren. Die GitHub-Pipeline ist die Referenz für die Systempakete. Unter Windows eine x64 Visual-Studio-Entwicklungsumgebung mit C++-Werkzeugen und Windows SDK verwenden.

```sh
pnpm install --frozen-lockfile
pnpm --filter @zuruf/ui build
pnpm --filter @zuruf/page build
# CEF_PATH auf einen Cache-Ordner setzen, beispielsweise "$PWD/.cef".
node tools/dev/fetch-cef.mjs "$(node tools/release/cef-version.mjs)" "$CEF_PATH"
cargo build --workspace --release --locked
cargo fetch --locked
python tools/release/package.py
```

Unter Windows den Downloadaufruf in PowerShell mit `$env:CEF_PATH` ausführen. NSIS muss für den Setup-Installer installiert sein. Unter Linux wird `dpkg-deb` für das Debian-Paket benötigt. Unter macOS reichen die Xcode-Befehlszeilenwerkzeuge und `brew install cmake ninja`. `--stage-only` erstellt nur das Laufzeitverzeichnis unter `.release-stage`, ohne Installer.

Für die Lizenzsammlung lädt das Skript eine festgelegte `cargo-about`-Version und verifiziert deren hinterlegte SHA-256-Prüfsumme. Rust-Lizenztexte, CEF-Lizenz, Chromium-Credits, Solid und beide Schriftlizenzen werden ins Paket aufgenommen.

## Prüfung

`python tools/release/test-installers.py` installiert in temporäre Verzeichnisse, prüft Aktualisierung und Deinstallation mit Datenerhalt und startet den tatsächlich paketierten Browser. Auch fremde Dateien im Installationsordner müssen erhalten bleiben. Der Stopp-Befehl wird mit geöffnetem Browser geprüft, damit Updates auf das Ende der Browserprozesse warten. Unter Linux ohne Desktop:

```sh
ZURUF_SHELL_ARGS=--disable-gpu xvfb-run -a -s '-screen 0 1600x1000x24' python tools/release/test-installers.py
```

Der native Test verwendet einen echten Daemon und Chromium, private temporäre Daten und einen lokalen HTTP-Prüfserver. Er prüft Start, Klicks, Screenshot, gefilterten Agentenzugang, Seitenbreite, Profilwiederherstellung und gezieltes Stoppen. Es werden keine echten Modellaufrufe, Mikrofonaufnahmen oder T3-Aufgaben ausgelöst. Windows und macOS werden auf eigenen Runnern gebaut und ausgeführt. Unter macOS installiert der Test die App aus dem Disk-Image, prüft Signatur und Helper-Bundles und startet den Browser mit `--use-mock-keychain`, damit der Schlüsselbund des Runners keinen Dialog öffnet. Die Transkriptionsdienste werden in Unit-Tests gegen einen lokalen Prüfserver geprüft.

Der Sitzungsstopp wird dreimal über den tatsächlichen „Stoppen“-Button im nativen Panel ausgelöst. Dabei darf dessen Clientverbindung mit dem alten Fenster enden: Die Sitzungsverwaltung muss das Hauptfenster unabhängig davon erhalten und beim erneuten Start wieder andocken. Der Prüfclient als Besitzer darf dafür den privaten Panel-Target untersuchen; der gefilterte Agentenzugang bleibt davon getrennt.

## Neuen Release erstellen

1. Änderungen per Pull Request prüfen und alle drei Plattform-Jobs abwarten.
2. Version in `Cargo.toml` und Lockdatei aktualisieren; `docs/release-notes.md` an den endgültigen Inhalt anpassen.
3. Geprüften Commit in `main` integrieren.
4. Den passenden Tag erstellen und pushen, beispielsweise `git tag v0.1.0` und `git push origin v0.1.0`.
5. Die Release-Pipeline baut erneut exakt den Tag. Ein fehlgeschlagener Build veröffentlicht keinen unvollständigen Release.

Die Veröffentlichung nutzt ausschließlich das kurzlebige GitHub-Actions-Token mit Schreibrecht im Publish-Job. Pull Requests und normale Builds erhalten nur Leserechte.
