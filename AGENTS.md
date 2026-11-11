# Zuruf – Projektanweisungen

Zuruf (Arbeitsname) ist ein lokales Review- und Orchestrierungswerkzeug: eigenes Browserfenster mit angedockter App, Annotationen per Hotkey (W/S/D), Voxtype-Diktat und asynchrone Übergabe von Feedback-Batches an T3 Code. Die verbindliche Produktspezifikation steht in `docs/spec.md`. Verifizierte Umgebungsfakten und Grenzen stehen unter `docs/feasibility/`, Architektur und Verträge in `docs/architecture.md` (sobald vorhanden).

## Harte Regeln

- T3 Code nie verändern, patchen oder dessen Datenbank anfassen. Nur über verifizierte externe Schnittstellen (`t3 thread …`, `t3 project …`, MCP) integrieren. Keine CLI-Befehle oder Endpunkte erfinden – erst lokal prüfen.
- Voxtype nicht verändern und dessen Konfiguration nicht dauerhaft umschreiben.
- Alle Server nur auf `127.0.0.1` binden. Keine Secrets, Cookies oder Browserprofile ins Repository.
- Release-Ziele sind Linux und Windows x64. Dieser Entwicklungsrechner nutzt Arch/Omarchy, **Hyprland auf Wayland** (nicht KDE), Rust stable über mise (`mise exec rust@stable -- cargo …`), Node 26 + pnpm nur für den Build von UI und Seiten-Agent. Der Browser ist Zurufs eigene Shell (`zuruf-shell`, eingebettetes Chromium über das `cef`-Crate; Build braucht `cmake` und `ninja`) – kein System-Chromium, keine Extension. Kein bun.
- Der Rechner hat 14 GB RAM und steht unter Speicherdruck. Browser, Dev-Server und Testläufe nur speicherbegrenzt starten: `systemd-run --user --wait --collect -p MemoryMax=3G -p MemorySwapMax=1G -- <befehl>`. Nie mehrere Browserfenster oder Dev-Server liegen lassen; eigene Prozesse am Ende beenden.
- Sprache: Code, Bezeichner und Commits englisch; UI-Texte deutsch (i18n-fähig halten); Dokumentation deutsch.
- Architektur ist auf Performance und geringen Speicherbedarf ausgelegt: Dienst und CLI (`zuruf`) und das Browserfenster (`zuruf-shell`) sind **Rust** (ein Cargo-Workspace, tokio, kein Node zur Laufzeit). Nur die Shell ist gegen Chromium gelinkt; die CLI bleibt klein. TypeScript strict nur dort, wo der Browser es erzwingt: Seiten-Agent (`apps/page`, ein eingebettetes Skript) und UI (pnpm-Workspace, kleine Bundles, vom Dienst ausgeliefert). Kein Electron. Lesbarer Code, keine verdichteten Einzeiler.
- `cargo build`/`cargo test` speicherbegrenzt und mit `-j 4` ausführen (`systemd-run … -p MemoryMax=4G`).
- Plattformabhängige Dateirechte, sichere Zufallswerte und Hintergrundprozesse über `zuruf-platform` führen. Windows nutzt private Besitzer-/SYSTEM-ACLs und den offiziellen CEF-Bootstrap mit Rust-DLL; die Sandbox nicht zum Vereinfachen von Releases abschalten. Diktat und desktopweite Hotkeys sind dort noch nicht verfügbar.
- Ohne Desktop (CI, fremder Rechner): Build und Tests im Container aus `tools/dev/` – dort laufen Shell und End-to-End-Test unter Xvfb oder einem Headless-Wayland-Compositor. `git` nur außerhalb des Containers benutzen.

## Rollen

- Produktführung, UX/UI, visuelles Design und das Frontend (`apps/ui`, Overlay-Styling) verantwortet der Projektleiter-Agent (Claude). Backend-Agenten liefern Funktion und Verträge, kein eigenes UI-Design.
- Jeder schreibende Agent arbeitet auf dem ihm zugewiesenen Branch/Worktree unter `../worktrees/` und committet selbst in kleinen, beschriebenen Commits.
