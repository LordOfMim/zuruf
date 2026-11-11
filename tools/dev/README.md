# Entwicklungsumgebung im Container

Baut und prüft Zuruf unter Linux, ohne Desktop und ohne lokal installierte Toolchain – etwa auf einem fremden Rechner oder in CI. Auf dem Zielrechner (Arch/Hyprland) braucht man das nicht; dort genügen `./install.sh` und `tests/e2e/run.sh`.

## Einrichten

```sh
docker build -t zuruf-dev tools/dev
docker run -d --name zuruf-dev --init --shm-size=1g --memory=10g \
  -v "$PWD:/work" -v zuruf-build:/build zuruf-dev sleep infinity
docker exec zuruf-dev bash -c 'id dev >/dev/null 2>&1 || useradd -m dev'
```

Build-Ergebnisse und Cargo-Cache liegen im Volume `zuruf-build` (`/build`), nicht im Checkout. **`git` nur außerhalb des Containers benutzen**: Im Container gelten andere Zeilenenden-Einstellungen, und ein dort ausgeführtes `git stash` oder `git add` erfasst dann den ganzen Baum.

Hinter einem Proxy, der TLS aufbricht, dessen Stammzertifikate als PEM nach `tools/dev/certs/` legen (wird nicht versioniert) und das Image neu bauen. Der CEF-Download des Builds benutzt den Zertifikatsspeicher des Systems nicht; dann vorab laden:

```sh
docker exec zuruf-dev node tools/dev/fetch-cef.mjs 156.0.26 /build/cef
```

Die Versionsnummer ist die hinter dem `+` in der Version des `cef`-Crates.

## Bauen und prüfen

```sh
run() { docker exec -e CEF_PATH=/build/cef zuruf-dev bash -c "cd /work && $*"; }
run 'pnpm install && pnpm --filter @zuruf/ui build && pnpm --filter @zuruf/page build'
run 'cargo build -j 8 --workspace && cargo build --release -j 8 --workspace'
run 'npm --prefix tests/mcp ci'
run 'Xvfb :99 -screen 0 1600x1000x24 & sleep 1; DISPLAY=:99 ZURUF_SHELL_ARGS="--no-sandbox --disable-gpu" bash tests/e2e/run.sh'
docker exec zuruf-dev su dev -c 'cd /work && CARGO_TARGET_DIR=/build/target bash tools/dev/e2e-wayland.sh'
```

`--no-sandbox --disable-gpu` gilt nur für den Container: Dort gibt es weder Benutzer-Namensräume für Chromiums Sandbox noch eine GPU.

## Hilfen

| Skript | Zweck |
|---|---|
| `run-shell.sh` | Shell unter Xvfb starten; Befehle nach `/tmp/shell-in` schreiben, Bildschirmfoto mit `import -window root` |
| `run-wayland.sh` | dasselbe unter sway (nicht als root); zeigt App-ID und Fenstertitel, wie der Compositor sie sieht |
| `e2e-wayland.sh` | `tests/e2e/shell.mjs` unter sway, einschließlich „Fenster schließen“ |
| `try-app.sh`, `api.sh` | echten Daemon mit Wegwerfdaten starten und seine API aufrufen |
| `measure.sh` | Release-Build installieren; Größe, Startzeit und Speicher ausgeben |
| `cdp.mjs` | DevTools-Befehle an eine laufende Shell schicken |
| `static-server.mjs` | die Prüfseiten aus `tests/browser/fixture` ausliefern |
| `fetch-cef.mjs` | Chromium-Laufzeit mit dem Zertifikatsspeicher des Systems laden |
