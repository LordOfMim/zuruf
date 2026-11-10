# Voxtype-Spike

`node spikes/voxtype/record-proof.mjs` prüft zuerst `idle`, startet etwa zwei Sekunden Dateiaufnahme und wartet mit `record stop --wait --json --timeout 30` auf Abschluss. `--busy-test` sendet während der eigenen Aufnahme einen zweiten Start mit derselben Zieldatei. `--cancel` verwirft die eigene Aufnahme. Pro Lauf ein neues temporäres Verzeichnis, kein Überschreiben vorhandener Transkripte. Eigene fehlgeschlagene Aufnahme wird im `finally` abgebrochen.

Nur bei unbenutztem Voxtype ausführen. Die Idle-Prüfung verhindert keine Race Condition mit der physischen Hotkey-Taste. Keine Konfigurationsänderung, kein Daemon-Neustart. Ergebnisse können bei Stille halluzinierten Text enthalten. Lokale CLI-Hilfe: `help.txt`.
