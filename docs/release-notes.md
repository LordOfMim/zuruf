Zurufs erste öffentliche Binärveröffentlichung: Browser-Reviews und gezielte Übergaben an Coding-Agenten in einer lokalen Desktop-App.

- Integrierter Chromium-Browser mit angedocktem Review-Werkzeug, ohne Extension oder Electron.
- Screenshots von Ansicht, Bereich oder Element; Kommentare mit URL, Elementkontext und Seitenfehlern.
- T3-Anbindung, konfigurierbare Tastenkürzel und sichtbare Browsersteuerung durch Agenten.
- Automatische Wiederherstellung bei Fensterschließen, Prozessabbruch oder Hängern; gespeicherte Profile und Tab-Adressen bleiben erhalten.
- Aufgaben lassen sich auch nach Abbruch oder Fehler abschließen und wieder öffnen.
- Windows-Setup und portables ZIP; Debian-Paket und fertiges Linux-Bundle.
- Beim Stoppen der letzten Browser-Sitzung bleibt Zuruf im Hauptfenster geöffnet.
- Neu: macOS-App für Apple silicon als Disk-Image und ZIP.
- Neu: Diktat mit wählbarem Dienst – Voxtype (Linux), whisper.cpp lokal oder online über OpenAI, Groq, Mistral, Deepgram, ElevenLabs und OpenAI-kompatible Server. API-Schlüssel werden nur lokal gespeichert und nicht wieder angezeigt.

**Plattformumfang:** Windows 10/11 und Linux, jeweils x64, sowie macOS 12+ auf Apple silicon. Diktat gibt es unter Linux und macOS, desktopweite Hotkeys nur unter Hyprland. T3, Modell- und Transkriptionsanbieter werden separat eingerichtet. Die Oberfläche ist deutsch. Der Windows-Installer ist noch nicht Authenticode-signiert, die macOS-App nicht notarisiert (erster Start einmal bestätigen).

Die Builds enthalten Chromium und Lizenzhinweise. SHA-256-Prüfsummen und GitHub-Build-Nachweise gehören zum Release. Die Pakete werden auf Linux-, Windows- und macOS-Runnern gebaut und mit dem tatsächlichen Browser geprüft. Das ist die erste Release-Serie; Fehlerberichte und Beiträge sind willkommen.

[Einrichtung und Bedienung](https://github.com/LordOfMim/zuruf#erste-schritte) · [Benutzerhandbuch](https://github.com/LordOfMim/zuruf/blob/main/docs/benutzerhandbuch.md) · [Fehler melden](https://github.com/LordOfMim/zuruf/issues/new/choose)
