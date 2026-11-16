Zuruf trennt jetzt sauber nach Projekten: Jedes Projekt hat eigene Startoptionen, Browser-Sitzungen und Browseroptionen. Dazu kommt ein neues Erscheinungsbild.

- **Startoptionen:** Lege je Projekt fest, was sich starten lässt: eine Adresse, auf Wunsch mit Startbefehl (etwa `pnpm dev`). „Starten“ führt den Befehl aus, wartet, bis die Adresse antwortet, und öffnet sie in einer eigenen Sitzung; „Stoppen“ beendet beides. Startbefehle lassen sich nur in der lokalen App anlegen, nie durch Agenten.
- **Allgemeine Projekte:** Ein Projekt braucht keinen Ordner mehr. So entsteht etwa ein Projekt „Allgemein“ für alles, was du immer wieder öffnest, oder „Prototypen“ mit allen Wegen, Prototypen zu starten. Jede Startoption kann ihren eigenen Ordner für Agenten mitbringen.
- **Browseroptionen je Projekt und Startoption:** Sitzungsart, Agentenzugriff und ein oder zwei Fenster – jeweils mit Rückfall auf Projekt und Einstellungen.
- **Neuer Bereich „Starten“:** Er ersetzt „Sitzungen“ und zeigt nur, was zum gewählten Projekt gehört.
- **Neues Design:** Die Oberfläche folgt dem Palermo-Designsystem, mit einem einheitlichen Icon-Satz und Geist Mono als Festbreitenschrift.

**Hinweise:** Das Speicherlimit für Startbefehle wirkt nur unter Linux mit systemd. Ohne Ordner bei Projekt und Startoption funktioniert das Review, „Senden“ ist dann gesperrt. Bestehende Projekte und Sitzungen bleiben unverändert erhalten.

**Plattformumfang:** Windows 10/11 und Linux, jeweils x64, sowie macOS 12+ auf Apple silicon. Diktat gibt es unter Linux und macOS, desktopweite Hotkeys nur unter Hyprland. T3, Modell- und Transkriptionsanbieter werden separat eingerichtet. Die Oberfläche ist deutsch. Der Windows-Installer ist noch nicht Authenticode-signiert.

Die Builds enthalten Chromium und Lizenzhinweise. SHA-256-Prüfsummen und GitHub-Build-Nachweise gehören zum Release. Die Pakete werden auf Linux-, Windows- und macOS-Runnern gebaut und mit dem tatsächlichen Browser geprüft.

[Einrichtung und Bedienung](https://github.com/LordOfMim/zuruf#erste-schritte) · [Benutzerhandbuch](https://github.com/LordOfMim/zuruf/blob/main/docs/benutzerhandbuch.md) · [Fehler melden](https://github.com/LordOfMim/zuruf/issues/new/choose)
