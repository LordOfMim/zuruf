Zuruf läuft jetzt auch auf dem Mac, und das Diktat arbeitet mit dem Transkriptionsdienst deiner Wahl.

- **macOS:** Neue App für Apple silicon (macOS 12 oder neuer) als Disk-Image und ZIP. Tastenkürzel mit `⌘`, eigene Menüleiste, Daten unter `~/Library/Application Support/Zuruf`.
- **Diktat mit wählbarem Dienst:** lokal mit Voxtype (Linux) oder whisper.cpp, online über OpenAI, Groq, Mistral, Deepgram, ElevenLabs oder einen eigenen OpenAI-kompatiblen Server. Die Einstellungen zeigen, ob der Ton den Rechner verlässt, und nennen fehlende Schlüssel oder Modelldateien.
- **API-Schlüssel bleiben privat:** Sie werden nur lokal und nur für dich lesbar gespeichert und nie wieder angezeigt. Stille wird nie verschickt.

**macOS, erster Start:** Die App ist ad hoc signiert, aber nicht von Apple notarisiert. Einmal unter *Systemeinstellungen › Datenschutz & Sicherheit* „Dennoch öffnen“ wählen oder `xattr -dr com.apple.quarantine /Applications/Zuruf.app` ausführen.

**Plattformumfang:** Windows 10/11 und Linux, jeweils x64, sowie macOS 12+ auf Apple silicon. Diktat gibt es unter Linux und macOS, desktopweite Hotkeys nur unter Hyprland. T3, Modell- und Transkriptionsanbieter werden separat eingerichtet. Die Oberfläche ist deutsch. Der Windows-Installer ist noch nicht Authenticode-signiert.

Die Builds enthalten Chromium und Lizenzhinweise. SHA-256-Prüfsummen und GitHub-Build-Nachweise gehören zum Release. Die Pakete werden auf Linux-, Windows- und macOS-Runnern gebaut und mit dem tatsächlichen Browser geprüft.

[Einrichtung und Bedienung](https://github.com/LordOfMim/zuruf#erste-schritte) · [Benutzerhandbuch](https://github.com/LordOfMim/zuruf/blob/main/docs/benutzerhandbuch.md) · [Fehler melden](https://github.com/LordOfMim/zuruf/issues/new/choose)
