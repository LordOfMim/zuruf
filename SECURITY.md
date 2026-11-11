# Sicherheitsprobleme melden

Bitte melde Sicherheitslücken über [GitHubs private Sicherheitsmeldungen](https://github.com/LordOfMim/zuruf/security/advisories/new). Veröffentliche keine funktionsfähigen Zugangsdaten oder fremden Daten in einem öffentlichen Issue.

Hilfreich sind die Zuruf-Version, Betriebssystem, nachvollziehbare Schritte, erwartete Auswirkung und eine Beschreibung der betroffenen Zugriffsgrenze. Ein kleines Beispiel mit synthetischen Daten reicht meist aus.

## Umgang mit Daten

- Dienste lauschen nur auf Loopback. Controller- und Agenten-Zugänge sind getrennt und widerrufbar.
- Lokale Dateien verwenden private Unix-Rechte beziehungsweise Windows-ACLs für Besitzer und SYSTEM. Administratoren desselben Rechners sind keine separate Sicherheitsgrenze.
- URLs, Screenshots, Sprachaufnahmen und Browserprofile können vertrauliche Inhalte enthalten. Ein Crash-Dump kann auch Tokens oder Passwörter enthalten. Bitte keine unbearbeiteten Dumps oder Profile hochladen.
- Browsersteuerung benötigt eine gültige Freigabe. Bereits laufende Aktionen können nach einer Übernahme noch kurz auslaufen; eine Übernahme ist kein Rollback.

Sicherheitsrelevante Korrekturen werden für den aktuellen veröffentlichten Stand bereitgestellt. Bei eingebettetem Chromium sind regelmäßige Updates wichtig; ältere Binärpakete erhalten keine eigenständige Wartungszusage.

Die Windows-Installer sind derzeit nicht Authenticode-signiert. Prüfsummen und Build-Nachweise auf der Release-Seite ordnen die Downloads ihrem Quellstand zu.
