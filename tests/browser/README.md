# Browser-Abnahme, 10.11.2026

Nachweis des Rust-Bausteins mit echter MV3-Extension und System-Chromium auf Wayland. Reproduzierbare Befehle und alle Grenzen: [Crate-README](../../crates/zuruf-browser/README.md). Bereinigte maschinenlesbare Messdaten: [evidence.json](evidence.json). Laufzeittoken, Cookies, Profile und Bilddateien werden nicht eingecheckt.

Bestanden im headed Labor: langlebiges Servercookie nach geordnetem Browserneustart (AT-01), derselbe sichtbare Tab über gefiltertes CDP mit `playwright-core` und stock `@playwright/mcp@0.0.68` (AT-02), Takeover/Release einschließlich laufendem Timer (AT-03), leerer Cookiebestand nach Wechsel auf frisches Profil (AT-04), Roh-/Ring-PNG (AT-05), pixelgenaue Region (AT-06), DOM inkl. AX-Anreicherung, offenem Shadow Root und last-pointer-frame-Routing bei Topframe-Tastaturfokus (AT-07). AT-17 prüft input, textarea, password, contenteditable, Shadow-Input, iframe-Input, select, Editorcontainer und fokussierten geschlossenen Custom-Element-Input. Untrusted DOM-Keyevents lösen keine Aufnahme aus. AT-19 ist nur für Attribut-Allowlist und konkret geprüfte Passwortmetadaten belegt, keine allgemeine Inhalts-/Bildredaktion.

Jeder Pixel der einfarbigen Region wird verglichen, nicht nur deren Abmessungen. Originalauswahl: 80×40 CSS-Pixel. DPR wird über CDP emuliert, Browserzoom über die echte Extension-API gesetzt:

| DPR | Zoom | PNG |
|---|---|---|
| 1 | 100 % | 80×40 |
| 1 | 125 % | 100×50 |
| 2 | 100 % | 160×80 |
| 2 | 125 % | 200×100 |

Release-Messung, sechs W-Aufnahmen, DPR 1/Zoom 100 %: Tastendruck bis Rohdatei **135, 207, 157, 172, 186, 170 ms**; bis Ringdatei bei Millisekundenauflösung dieselben Werte. Median **171 ms**, Spanne **135–207 ms**. **Das Ziel deutlich unter 150 ms ist nicht erreicht.** Der erste Hotkey öffnet das Panel; dadurch wechselt der Viewport im Lauf von 922×1063 auf 536×1055. Keine p95-Aussage, kein Dauerlasttest, kein fsync. Das Labor schreibt das Rohbild erst nach Rückkehr der kompletten Capture-Pipeline; eine zweistufige API für frühe Rohbildpersistenz ist noch erforderlich. Diese Zahlen beziehen sich auf die getestete Maschine unter laufender Desktoplast, nicht auf eine UI-Ende-zu-Ende-Latenz im noch nicht verdrahteten Daemon.

Takeover-Race: Ein bereits laufender 900-ms-JavaScript-Timer wird nach etwa 100 ms übernommen. Neue Befehle werden während `settling` abgewiesen. Der Takeover-HTTP-Aufruf kehrt nach **514 ms** zurück; der Timer verändert das DOM nach insgesamt **903 ms** trotzdem. Das entspricht der Entscheidung `Page.stopLoading` nach 500 ms ohne `Runtime.terminateExecution`; `settling:false` ist kein Beweis, dass fremdes JavaScript vollständig gestoppt ist.

Zehn Rust-Tests prüfen Framing, Clipmatrix, Ring-Goldenpixel, Redaktion, Filter, Lease/Release, Grant-Generationsrennen, Extension-Origin-/Tokenprüfung, Reconnect/Dedup und FIFO-/Takeover-Verhalten mit echtem WS-Mockbackend. TypeScript strict, ESLint und Prettier sowie Rust fmt/Clippy mit `-D warnings` sind Teil der dokumentierten Checks. Ein separater ignorierter Integrationstest startet das vorgebaute Labor explizit.

Nicht vollständig abgenommen: echter menschlicher Login/2FA, reale IME/Editoren, Panelpixel/Fokus/physische Hotkeys, Takeover mitten in Drag/Navigation, Monitor-Mischskalierung, Cross-Origin/OOPIF-/transformierte Frames, Store/chrome/PDF-Sonderseiten, erzwungener Worker-Schlaf, Crash-Recovery unter Last. Die automatisierte Profilisolation benutzt nacheinander gestartete Browser; sie beweist keine gleichzeitige Mehrsession-Produktintegration.
