# Zu Zuruf beitragen

Fehlerberichte, konkrete UX-Verbesserungen und Pull Requests sind willkommen. Für größere Änderungen bitte zuerst ein Issue mit Problem, gewünschtem Verhalten und betroffener Plattform öffnen.

## Entwicklung

- Rust-Toolchain aus `rust-toolchain.toml`, Node 26, pnpm aus `package.json` und CMake/Ninja verwenden.
- `pnpm install --frozen-lockfile`, dann UI und Seiten-Agent bauen. Anschließend `cargo build --workspace --locked`.
- Unter Windows benötigt die Shell CEFs Sandbox-Bootstrap und die Rust-DLL. [Bundle-Anleitung](docs/releases.md).
- Die App-Daten für Tests immer über `ZURUF_DATA_DIR`, `ZURUF_STATE_DIR`, `ZURUF_CONFIG_DIR` und einen eigenen `ZURUF_PORT` isolieren. Niemals ein persönliches Browserprofil für Tests verwenden.
- Code, Bezeichner und Commits sind englisch; Oberfläche und Dokumentation deutsch.

## Prüfen

```sh
pnpm --filter @zuruf/ui build
pnpm --filter @zuruf/page check
pnpm --filter @zuruf/page build
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Die GitHub-Pipeline baut und testet Linux und Windows. Die Release-Prüfung startet einen echten Browser mit temporären Daten und ohne Modellaufruf. Unter Linux laufen die Desktopprüfungen auf dem Runner unter Xvfb. Tests mit echten Agenten oder Mikrofon sind ausdrücklich gesondert markiert und sollen nicht unbeabsichtigt ausgeführt werden.

Auf speicherknappen Linux-Rechnern Builds und Tests mit `systemd-run --user --wait --collect --pipe -p MemoryMax=4G -p MemorySwapMax=1G -- …` begrenzen und Cargo mit `-j 4` ausführen.

## Pull Requests

Beschreibe das konkrete Problem, das neue Verhalten und die ausgeführten Prüfungen. Bilder bitte mit Beispieldaten erstellen. Zugangsdaten, persönliche Projekte, Browserprofile und unveränderte Crash-Dumps gehören nicht in einen PR. T3 Code und Voxtype werden ausschließlich über ihre externen Schnittstellen angebunden und nicht gepatcht.

Mit einem Beitrag stimmst du zu, ihn unter Zurufs MIT-Lizenz bereitzustellen. Drittanbieter-Lizenzen müssen erhalten bleiben.
