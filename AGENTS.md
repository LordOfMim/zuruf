# Zuruf – Projektanweisungen

Zuruf (Arbeitsname) ist ein lokales Review- und Orchestrierungswerkzeug: verwalteter Chromium, Annotationen per Hotkey (W/S/D), Voxtype-Diktat und asynchrone Übergabe von Feedback-Batches an T3 Code. Die verbindliche Produktspezifikation steht in `docs/spec.md`. Verifizierte Umgebungsfakten und Grenzen stehen in `docs/feasibility.md`, Architektur und Verträge in `docs/architecture.md` (sobald vorhanden).

## Harte Regeln

- T3 Code nie verändern, patchen oder dessen Datenbank anfassen. Nur über verifizierte externe Schnittstellen (`t3 thread …`, `t3 project …`, MCP) integrieren. Keine CLI-Befehle oder Endpunkte erfinden – erst lokal prüfen.
- Voxtype nicht verändern und dessen Konfiguration nicht dauerhaft umschreiben.
- Alle Server nur auf `127.0.0.1` binden. Keine Secrets, Cookies oder Browserprofile ins Repository.
- Zielumgebung ist dieser Rechner: Arch/Omarchy, **Hyprland auf Wayland** (nicht KDE), Node 26, pnpm, System-Chromium unter `/usr/bin/chromium`. Kein bun, kein cargo.
- Der Rechner hat 14 GB RAM und steht unter Speicherdruck. Browser, Dev-Server und Testläufe nur speicherbegrenzt starten: `systemd-run --user --wait --collect -p MemoryMax=3G -p MemorySwapMax=1G -- <befehl>`. Nie mehrere Chromium-Instanzen oder Dev-Server liegen lassen; eigene Prozesse am Ende beenden.
- Sprache: Code, Bezeichner und Commits englisch; UI-Texte deutsch (i18n-fähig halten); Dokumentation deutsch.
- TypeScript strict, pnpm-Workspace. Lesbarer Code, keine verdichteten Einzeiler.

## Rollen

- Produktführung, UX/UI, visuelles Design und das Frontend (`apps/ui`, Overlay-Styling) verantwortet der Projektleiter-Agent (Claude). Backend-Agenten liefern Funktion und Verträge, kein eigenes UI-Design.
- Jeder schreibende Agent arbeitet auf dem ihm zugewiesenen Branch/Worktree unter `../worktrees/` und committet selbst in kleinen, beschriebenen Commits.
