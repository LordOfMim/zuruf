
# Local Browser Review & AI Agent Orchestrator

> **Produktspezifikation / Implementierungsauftrag für einen Coding-Agenten**  
> **Stand:** 10. November 2026  
> **Status:** Entwurf, technische Details zu T3 Code und Voxtype bei Implementierungsbeginn prüfen  
> **Zielplattform:** Linux-Desktop (Wayland/X11; Entwicklung auf Arch/Hyprland) und Windows 10/11 x64
> **Lizenzziel:** Open Source, lokale Ausführung, keine verpflichtenden Cloud-Dienste

## 1. Kurzbeschreibung

Entwickle eine **eigenständige lokale Desktop-/Browser-Applikation**, die einen gemeinsam durch Benutzer und KI-Agent steuerbaren Webbrowser, visuelle Annotationen, Voxtype-Spracherkennung und die **asynchrone Übergabe mehrerer paralleler Entwicklungsaufgaben an T3 Code** vereint.

Die Anwendung ist ein **Review- und Orchestrierungswerkzeug**, **kein Ersatz für T3 Code**. T3 Code bleibt unverändert. Die neue Applikation interagiert ausschließlich über vorhandene, tatsächlich verfügbare Schnittstellen mit der lokal laufenden T3-Code-Instanz. Sie soll möglichst wenig manuelle Interaktion erfordern und auf dem eigenen Rechner laufen. Veröffentlichungen enthalten Installer für Linux und Windows; Diktat über Voxtype/PipeWire und desktopweite Hotkeys über Hyprland bleiben zunächst Linux-Funktionen.

**Kernidee:** Ich navigiere durch meine Web-App, spreche Änderungswünsche frei ins Mikrofon und drücke währenddessen `W`, `S` oder `D`, um den aktuellen Bildschirm, einen Ausschnitt oder ein DOM-Element mit dem Gesagten zu verknüpfen. Über `Shift+N` wird das bisher Gesammelte sofort als **neuer T3-Thread** an einen einstellbaren Coding-Agenten übergeben, während ich **ohne Unterbrechung** weitere Aufgaben für andere Agenten aufnehmen kann.

### 1.1 Nicht verhandelbare Leitplanken

1. **T3 Code nicht verändern, forken oder patchen.** Ausschließlich externe Integration.
2. **Local-first.** Browser, Feedback-Daten, Screenshots, Orchestrierung und Transkription grundsätzlich lokal. Externe KI-Provider sind nur über die bereits konfigurierte T3-Umgebung beteiligt.
3. **Derselbe Browser für Mensch und Agent**, wenn Shared Mode ausgewählt ist; daneben sauber isolierte Sessions.
4. **Review-Aufnahme darf durch Task-Dispatch nie blockieren.** Neue Aufgaben lassen sich starten, während ältere noch laufen.
5. **Kontextreiche Annotationen statt bloßer Screenshots:** URL, Tab, Session, DOM, Position, Sprache, Zeitstempel, Screenshot, ggf. Entwicklungsprojekt.
6. **Provider/Modell/Arbeitsmodus in der neuen App wählbar**, soweit die verbundene T3-Version sie unterstützt.
7. **Mehrere Sessions und Aufgaben parallel**, mit klar nachvollziehbarer Zuordnung von Browser, Feedback-Batch und T3-Thread.

---

## 2. Beispiel-Workflow / User Journey

1. Ich starte die lokale App und wähle ein Entwicklungsprojekt mit Repository-Pfad und Start-URL, etwa `http://localhost:3000`.
2. Ich öffne eine persistente Chromium-Browsersitzung und logge mich nötigenfalls **manuell** in die Webanwendung ein (inklusive 2FA). Die Session kann anschließend auch von einem Agenten bedient werden.
3. Im **Review-Modus** spreche ich durch Voxtype und navigiere mit Maus und Tastatur.
4. Ich drücke `W`: Ein Screenshot des sichtbaren Browser-Viewports entsteht, der Cursorbereich wird durch einen klar sichtbaren Kreis hervorgehoben. URL, Tab, Scrollposition, Cursorposition und Sprachkontext werden gespeichert.
5. Ich drücke `S`: Ich ziehe einen rechteckigen Ausschnitt auf. Der Ausschnitt wird als Annotation erfasst und mit der dazugehörigen Sprache verknüpft.
6. Ich drücke `D`: Das gerade unter dem Mauszeiger liegende **DOM-Element** wird ausgewählt, visuell hervorgehoben und mit robustem Locator, Elementdaten, Screenshot und gesprochenem Kommentar gespeichert.
7. Ich kann mehrere solche Markierungen machen, ohne die Aufnahme jedes Mal komplett verlassen zu müssen. Die App erzeugt **getrennte, nachträglich editierbare Feedback-Einträge**.
8. Ich drücke `Shift+N`: Der aktuelle Aufgabenpuffer wird **atomar abgeschlossen**, mit Modell/Provider/Projekt verknüpft und **nicht blockierend** an einen **neuen T3-Thread** gesendet. Ein neuer leerer Puffer wird sofort aktiv.
9. Während Agent A Aufgabe 1 bearbeitet, spreche ich bereits Aufgabe 2 ein und starte Agent B – bei Bedarf mit anderem Modell oder in einem anderen Worktree.
10. Eine lokale Aufgabenübersicht zeigt, welcher Thread welche Annotation bearbeitet, ob er noch läuft und ob eine Antwort, ein Fehler, eine Freigabe oder ein Konflikt vorliegt.
11. Ich kann einen Task nachträglich kommentieren, erneut an denselben Thread senden, in einen neuen Thread aufteilen oder neu aufnehmen.

**Wichtiges Verhalten beim Dispatch während laufender Diktierung:** `Shift+N` markiert eine **saubere sprachliche Segmentgrenze**. Bereits zuordenbare Aufnahme-/Transkriptabschnitte gehören zum abgeschlossenen Batch; danach Gesprochenes gehört zum neuen Batch. Wenn Voxtype nur nach Stopp eine Transkription liefert, darf der Sendestatus temporär `finalizing_transcript` lauten. Die UI schaltet dennoch sofort auf einen neuen Puffer um. Ein Auftrag wird erst mit vollständig finalisiertem Transkript tatsächlich versendet oder klar als unvollständig ausgewiesen. Nichts darf verloren gehen oder irrtümlich im falschen Thread landen.

---

## 3. Eingabekonzept und Shortcuts

Die Tastenbelegung ist eine **konfigurierbare Voreinstellung**. Einfache Buchstabentasten funktionieren **nur im explizit aktivierten Review-Modus und nur außerhalb von Texteingaben**. In Textfeldern, Passwortfeldern, Code-Editoren und Content-Editable-Bereichen bleiben `W`, `S`, `D` normale Eingaben. Alternativ müssen Modifier-Kombinationen oder native KDE-Shortcuts möglich sein.

| Shortcut | Aktion | Ergebnis |
|---|---|---|
| `W` | Viewport-Capture | Screenshot der aktiven Webseite mit Ring/Kreis um Cursor und sichtbarem Kontext |
| `S` | Region-Capture | Rechteck per Maus aufziehen; Ausschnitt und Koordinaten erfassen |
| `D` | DOM-Capture | DOM-Element unter dem Cursor auswählen, Locator und Elementmetadaten speichern |
| `Shift+N` | **Neuen Agenten-Task starten** | Aktuellen Batch abschließen, neuen T3-Thread asynchron starten, sofort neuen Batch öffnen |
| `R` | Aufnahme starten/pausieren | Review mit Sprachtranskription starten/pausieren |
| `E` | Textkommentar | Zusätzlichen manuellen Text am aktuellen Element/Batch hinterlegen |
| `Esc` | Aktion abbrechen | Laufende Auswahl beenden, **ohne** vorhandene Annotationen zu verlieren |
| Konfigurierbar | An bestehenden Thread senden | Ausgewählte Annotation als Follow-up an bestimmten T3-Thread schicken |
| Konfigurierbar | Agentensteuerung pausieren | Browser-Kontrolle sofort an den Benutzer zurückgeben |

**Interaktion bei `W`, `S`, `D`:** Es muss konfigurierbar sein, ob eine Aktion (a) einen neuen Feedback-Eintrag anlegt, (b) den aktuell offenen Eintrag erweitert oder (c) nach der Aufnahme einen neuen Spracheintrag startet. Standard: **neuer Eintrag mit eindeutigem Zeitstempel**; bei laufender Diktierung werden Sprachsegmente anhand dieser Marker abgegrenzt.

**Aufnahmestatus sichtbar:** `Idle`, `Recording`, `Transcribing`, `Reviewing`, `Dispatching`, `Error`. Overlay, Tray-Indikator oder kompakte Statusanzeige mit eindeutiger Rückmeldung.

---

## 4. Anforderungen an Browser und Sessions (P0)

### 4.1 Grundlegende Browsersteuerung

- Lokale und entfernte URLs öffnen; mehrere Tabs und Fenster unterstützen.
- Mensch kann normal klicken, tippen, scrollen, einloggen und navigieren.
- KI-Agent kann über MCP/Playwright dieselben Aktionen ausführen.
- Agent sieht den korrekten Tab, die URL, DOM-/Accessibility-Informationen und Screenshots.
- **MCP-Tools sollen eine Session-ID oder einen gekapselten Session-Endpunkt nutzen.** Keine versehentliche Steuerung fremder Sitzungen.
- Tabs einer Session werden nach Möglichkeit wiederhergestellt.
- Login-Zustände bleiben in persistenten Profilen erhalten, soweit Browser und Webseite dies zulassen.
- Sensible Logins und MFA erfolgen mit menschlicher Übernahme; kein Umgehen von MFA.

### 4.2 Session-Typen

| Modus | Verhalten | Typischer Einsatz |
|---|---|---|
| **Shared** | Benutzer und Agent steuern denselben Browser-Kontext und ggf. denselben Tab | Gemeinsames Debugging, Agent führt Schritte sichtbar aus |
| **Isolated** | Separates Browserprofil bzw. isolierter Browser-Kontext, eigene Cookies/Storage | Unabhängige Tests, anderer Login, parallele Agenten |
| **Observer** | Benutzer beobachtet Agenten-Session; kann per Takeover übernehmen | Agentenlauf live verfolgen |

- Sessions benennen, erstellen, duplizieren, pausieren, fortsetzen und schließen.
- **Session-Split** unterscheiden zwischen: (1) neuem Tab im bestehenden Browser-Kontext, (2) isoliertem neuen Kontext, (3) explizitem Klon ausgewählter Authentifizierungsdaten – nur mit Freigabe, standardmäßig **nicht** automatisch.
- Session-, Tab-, Projekt- und Agentenbindung getrennt modellieren.
- Eine geteilte Session darf für verschiedene Reviews genutzt werden, ohne Review-Daten zu vermischen.

### 4.3 Kollisions- und Kontrollmanagement

Gemeinsame Browsersteuerung muss deterministisch sein:

- `control_owner` = `human | agent:<id> | none` mit Lease/Timeout.
- Sichtbare Anzeige, wer die Kontrolle hat.
- Manuelles Takeover hat Vorrang; Agentenaktionen werden dann ausgesetzt bzw. sicher abgebrochen.
- Gleichzeitige Klick-/Navigationsaktionen in demselben Tab werden serialisiert oder zurückgewiesen.
- Unterschiedliche Tabs dürfen parallel genutzt werden, wenn technisch sicher.
- Ein Agent darf ohne explizite Berechtigung keine Session übernehmen.

### 4.4 Umsetzungspräferenz

**Keinen eigenen Browser-Engine-Stack entwickeln.** Bevorzugt ein lokal gestarteter **Chromium** mit Playwright-/CDP-Steuerung und einer Browser-Extension bzw. Content-Script-Overlay für DOM-Annotationen. Das lokale UI kann als Companion-Anwendung (Web-UI in localhost und optional Desktop-Shell) umgesetzt werden. Prüfe, ob Playwright MCP mit Extension-Modus oder CDP an den **tatsächlich selben sichtbaren Browser** gekoppelt werden kann. Browser-Automation, Overlay und menschliche Interaktion müssen nachweislich dieselben Tab-IDs und denselben DOM-Zustand nutzen.

---

## 5. Capture-System und Annotationen (P0)

### 5.1 W – Viewport-Screenshot mit Cursormarker

- Screenshot **des sichtbaren Browser-Viewports** im aktuellen Zustand erzeugen.
- Cursorposition in **Viewport-Koordinaten** erfassen (DPI-/Zoom-Faktor beachten).
- **Kreis/Ring um den Cursor** in einer separaten, finalen Bilddatei rendern; zusätzlich rohe, unmarkierte Bilddatei speichern, wenn gewünscht.
- Der Ring soll den relevanten UI-Bereich nicht vollständig verdecken; Größe/Farbe konfigurierbar.
- Aktuelle URL, Titel, Scrollposition, Viewport-Maße, Zeitstempel, Projekt/Session/Tab speichern.
- Optional: aktuelle Accessibility-/DOM-Snapshot-Referenz und Element unter Cursor.

### 5.2 S – Ausschnitt

- Nutzer zieht im sichtbaren Browser eine Auswahl auf.
- Zuschnitt als Bild sichern; Original-Rechteck in Viewport-Koordinaten speichern.
- Bei Bedarf zusätzlich kompletten Viewport für Orientierung sichern.
- Overlay muss nach Erstellung verschwinden und keine Seitenelemente dauerhaft verändern.

### 5.3 D – DOM-Element auswählen

- Hover über Elemente zeigt Auswahlrahmen.
- Bei Druck auf `D` wird das **unter dem Cursor befindliche** DOM-Element erfasst; alternativ expliziter Element-Picker, falls Fokus/Wayland dies erzwingt.
- Speichern: CSS-Locator, XPath nur als Fallback, Playwright-Locator nach Rolle/Name/Text, Tag, Attribute (mit Redaktionsregeln), sichtbarer Text, Accessibility-Rolle/Name, Bounding Box, relevante Eltern-/Kind-Informationen, URL, Screenshot.
- Bevorzuge **stabile Locator-Kandidaten**: `data-testid`, `aria-*`/Role+Name, eindeutiger Text; instabile CSS-Selektoren kenntlich machen.
- Wenn das Element in Shadow DOM/Iframe liegt: bestmöglichen Frame-/Shadow-Kontext erfassen, Grenzen dokumentieren.
- Sensible Eingabewerte, Passwortfelder, Tokens, Session-Cookies niemals ungefiltert speichern.
- Annotationen bei späterer Seitenöffnung nach Möglichkeit rekonstruieren; bei fehlendem Match alte Aufnahme anzeigen, **keine falsche DOM-Zuordnung** vortäuschen.

### 5.4 Optionale visuelle Werkzeuge (P1)

Pfeil, Kreis, Freihandzeichnung, Textmarker, mehrere Markierungen pro Screenshot und Kommentare anderer Teilnehmer. Alle Änderungen nicht destruktiv auf Layern speichern.

### 5.5 Jeder Feedback-Eintrag enthält

- ID, Ersteller, Erstellungs-/Änderungszeit
- Projekt, Review, Batch, Session, Tab
- URL (inklusive Path/Query nach Redaktionsregeln), Seitentitel, Route
- Typ `viewport | region | dom | text`
- Screenshot-Dateien und Koordinaten/Locatoren
- Verknüpfte Sprachsegmente und editierbarer Transkripttext
- Manuelle Anmerkungen, Priorität, Tags, Status
- T3-Thread-/Task-Zuordnung nach Dispatch
- Optionaler Git-Commit/Worktree-Kontext, sofern verfügbar

---

## 6. Voxtype und Spracherkennung (P0/P1)

### 6.1 Grundprinzip

- Nutze **bestehendes lokal installiertes Voxtype**, keine eigene Spracherkennung für das MVP.
- Ein `SpeechAdapter` kapselt Erkennung, Start/Stop, Dateiausgabe, Status, Fehler und optional Meeting-/Streaming-Modus.
- Nutzer kann gesprochene Texte **vor und nach** der Übergabe korrigieren.
- Keine unkontrollierte Eingabe des Transkripts in das gerade aktive Browser-Textfeld: Ausgabe ausdrücklich in eine temporäre lokale Datei bzw. den App-Speicher routen.

### 6.2 Nach aktuellem Stand bekannte CLI-Anbindung

Laut Voxtype-Dokumentation existieren u. a.:

```bash
voxtype record start --file=/tmp/review-annotation-001.txt
voxtype record stop
voxtype record cancel
```

Die Dateiausgabe verwendet in dieser Dokumentation **`--file=...`**, nicht `--output-file`. Vor Integration **lokale Version und `voxtype --help` prüfen**; Schreibabschluss der Datei abwarten, atomar einlesen und Fehler melden. Die verfügbaren Meeting-/Segmentierungsfunktionen versioniert prüfen.

### 6.3 Sprachliche Zuordnung

Die App soll **nicht** einfach ein langes Transkript beliebig auf Screenshots verteilen.

- Jedes `W`/`S`/`D` erzeugt einen Zeit-/Abschnittsmarker.
- MVP: verlässliche, **explizite Segmentgrenzen** durch Start/Stop oder durch Capture-Aktionen. Sprachaufnahme darf jeweils einem eindeutigen Capture zugeordnet werden.
- P1: kontinuierliches Review mit Transkriptsegmenten und Zeitstempeln. Einbindung über Voxtype-Meeting-Modus, falls dieser die benötigten Zeitinformationen bereitstellt; sonst alternative lokale Aufnahme-/Segmentierungsstrategie **ohne Änderung an Voxtype**.
- Optionale KI-Segmentierung kann Abschnitte semantisch glätten, darf aber den ursprünglichen Text und seine Zuordnung nicht überschreiben.
- Mehrere Captures können bewusst denselben Sprachabschnitt referenzieren. Ambivalente Zuordnungen müssen in der UI korrigierbar sein.
- Audiomitschnitt nur optional und mit expliziter Einstellung speichern; Standard sind Transkript und Metadaten.

### 6.4 Shift+N während einer Aufnahme

- `Shift+N` erzeugt eine **neue Epoch-/Batch-ID** zu einem präzisen Zeitstempel.
- Vorherige Captures und Sprachanteile verbleiben im alten Batch.
- Der alte Batch wird mit Status `finalizing_transcript` abgeschlossen, bis die benötigten Segmente vorliegen.
- Der neue Batch wird **unmittelbar** aktiv, selbst wenn noch Transkription/Dispatch des alten Batches läuft.
- Falls Voxtype nur eine einzelne aktive Aufnahme pro Daemon erlaubt: Adapter implementiert eine **sichere Stop-/Finalize-/Restart-Sequenz**. Die potenzielle kurze Audio-Lücke wird als technische Grenze ausgewiesen, nicht verschwiegen. Alternativ ist eine dedizierte kontinuierliche Audio-Pipeline zu prüfen.
- Versand geschieht erst nach konsistentem Snapshot; bei Fehlern kann man manuell korrigieren und erneut versenden.

---

## 7. Aufgabenpuffer, Parallelität und Routing (P0 – Kernfunktion)

### 7.1 Konzepte

- **Review:** Eine gesamte Feedback-Sitzung.
- **Capture/Annotation:** Eine spezifische, kontextreiche Markierung mit Text/Audio.
- **Active Batch:** Aktuell gesammelte, noch nicht abgeschickte Annotationen und Freitext.
- **Dispatched Batch:** Unveränderlicher Snapshot des abgesendeten Inhalts; spätere Änderungen erzeugen eine neue Revision oder ein Follow-up.
- **T3 Task:** Verknüpfung zu einem konkreten T3-Thread und dessen Laufstatus.
- **Agent Profile:** Projekt, Provider, Modell, Reasoning-/Runtime-/Approval-Modus und Workspace-Strategie.

### 7.2 Verhalten von `Shift+N` (unbedingt umsetzen)

1. Erfasse atomar `batchId`, alle aktuellen Annotationen, freie Sprachteile und Seiten-/Repo-Kontext.
2. Setze diesen Batch auf `sealed` bzw. `finalizing_transcript`; danach **keine stillen Mutationen** mehr.
3. Erstelle **sofort einen neuen leeren Batch** mit neuem Sprachsegment-/Capture-Kontext.
4. Validiere die Dispatch-Voraussetzungen und finalisiere ggf. verbleibende Transkription **asynchron**.
5. Erzeuge einen neuen T3-Thread mit dem **für den alten Batch eingefrorenen Agent Profile** und strukturiertem Kontext.
6. Speichere `threadId`, Provider-/Modellwahl, Workspace-Pfad, Versandzeit, Auftrags-ID und Status.
7. Zeige Erfolg oder Fehler in der Task-Liste, **ohne die Review-Aufnahme zu unterbrechen**.
8. Bei Fehler bleibt der abgeschlossene Batch erhalten und ist per Retry **idempotent** erneut versendbar; kein doppelter Thread bei unklarem Netzwerkstatus.

**Nicht zulässig:** Nur den Text in die Zwischenablage kopieren, T3 Code in den Vordergrund zwingen, bis zum Agentenende blockieren oder Screenshots/DOM-Kontext verlieren.

### 7.3 Routing

- Neues T3-Thread-Fenster/Thread-Objekt je abgeschlossenem Batch, soweit ausgewählt.
- Alternativ Follow-up an einen vorhandenen Thread.
- Ein Modell/Provider als Default pro Projekt und optional je Batch/Hotkey-Profil auswählbar.
- Anzeige tatsächlich verfügbarer Provider und Modelle aus der verbundenen T3-Instanz, **keine statisch erfundene Liste**.
- Optional `Shift+1` / `Shift+2` für Agentenprofile oder Modell-Presets; keine Pflicht im MVP.
- Mehrere gleichzeitig laufende Tasks; keine künstliche globale Sperre.
- Fehler/Abbruch/Freigabeanforderung und Agentenantwort im eigenen UI anzeigen, soweit Schnittstelle dies liefert.
- In der UI können abgeschlossene Aufgaben manuell als erledigt markiert und mit T3-Antworten korreliert werden.

### 7.4 Schutz vor Codekonflikten

**Separate Threads sind nicht automatisch separate Arbeitsverzeichnisse.** Für gleichzeitig schreibende Agenten in demselben Repository:

- Bevorzugt **einen eigenen Git-Worktree pro parallelem Schreibauftrag**, sofern T3 die entsprechende Workspace-Strategie unterstützt.
- Für gemeinsame Checkout-Pfade zumindest Warnung und optionaler Schreib-Lock.
- Beim Dispatch `workspaceStrategy` sichtbar auswählen: `new_worktree | existing_worktree | shared_root`.
- Abhängige Aufgaben können bewusst nacheinander ausgeführt werden.
- Zusammenführung/Cherry-Picking ist ein separater, expliziter Schritt, keine stillschweigende Auto-Merge-Funktion.

---

## 8. Integration in T3 Code (P0)

### 8.1 Grundsatz

Die Anwendung ist **ein externer Client/Controller** für T3 Code. **Keine Modifikation** des T3-Code-Codes, keine Browser-Simulation der T3-Oberfläche als primäre Integrationsmethode, kein direktes Schreiben in interne T3-SQLite-Datenbanken.

Die zu implementierende Adapter-Schnittstelle lautet sinngemäß:

```ts
interface T3Adapter {
  discoverCapabilities(): Promise<T3Capabilities>;
  listProjects(): Promise<ProjectInfo[]>;
  listProvidersAndModels(projectId?: string): Promise<ModelChoice[]>;
  createThread(input: {
    projectPath: string;
    title: string;
    prompt: string;
    providerId?: string;
    modelId?: string;
    runtimeMode?: string;
    reasoningEffort?: string;
    workspaceStrategy?: 'new_worktree' | 'existing_worktree' | 'shared_root';
    clientRequestId: string;
  }): Promise<{ threadId: string; resolvedWorkspacePath?: string }>;
  sendMessage(threadId: string, prompt: string): Promise<void>;
  getThreadStatus(threadId: string): Promise<ThreadStatus>;
  getThreadMessages?(threadId: string): Promise<ThreadMessage[]>;
  interruptThread?(threadId: string): Promise<void>;
}
```

**Das ist die gewünschte Abstraktion, keine Behauptung über die echten T3-Endpunktnamen.** Zuerst verfügbare externe Schnittstellen und Autorisierung der installierten T3-Version prüfen. Adapter mit Capability Detection und sauberem Fallback implementieren. Ein Drittanbieter-CLI-Wrapper ist nur nach expliziter Bewertung sinnvoll; nicht ungefragt als native T3-Funktion ausgeben.

### 8.2 Integrationsreihenfolge

1. Tatsächlich verfügbare, dokumentierte T3-MCP-Tools oder offiziell vorgesehene lokale Client-/Orchestrierungs-Schnittstelle prüfen.
2. Authentifizierung, Projektauswahl, Modell-Discovery, Thread-Erstellung und Event-/Statusabfrage im kleinen Test bestätigen.
3. Wenn keine geeignete stabile externe Schnittstelle vorhanden ist: einen externen Adapter gegen nachweislich verfügbare Schnittstellen bauen und Versionskompatibilität dokumentieren; **T3 Code bleibt unverändert**.
4. Fehlt ein erforderlicher Zugriff, genau diesen als Blocker melden, **keine erfundene CLI/API implementieren**.

T3 dokumentiert u. a. Thread-/Orchestrierungsfunktionen und `t3` als Launcher. Aus der Existenz von `t3` folgt **nicht**, dass Kommandos wie `t3 thread start` oder `t3 thread send` in der installierten Version existieren. Die Implementierung muss das prüfen.

### 8.3 Übergabeformat an den Coding-Agenten

Je Batch wird ein lokales Manifest mit stabilen **relativen** Asset-Pfaden erzeugt und ein Prompt übergeben, der diese Dateien eindeutig referenziert. Der Agent muss die Manifest- und Bilddateien im tatsächlich erreichbaren Arbeitskontext lesen können. Falls der T3-Provider binäre Attachments unterstützt, kann diese Funktion zusätzlich genutzt werden.

Beispiel:

```json
{
  "schemaVersion": "1.0",
  "batchId": "batch-20261110-001",
  "project": {
    "name": "Web-App",
    "repositoryPath": "/home/user/projects/web-app",
    "baseUrl": "http://localhost:3000"
  },
  "browserContext": {
    "sessionId": "session-abc",
    "tabId": "tab-2",
    "url": "http://localhost:3000/dashboard",
    "title": "Dashboard",
    "viewport": { "width": 1440, "height": 900, "deviceScaleFactor": 1 }
  },
  "annotations": [
    {
      "id": "ann-001",
      "type": "dom",
      "transcript": "Der Button soll nach dem Speichern eine Bestätigung anzeigen.",
      "captureTime": "2026-11-10T16:00:00+01:00",
      "screenshot": "assets/ann-001.png",
      "locator": {
        "role": "button",
        "name": "Speichern",
        "css": "[data-testid='save-button']"
      },
      "boundingBox": { "x": 100, "y": 150, "width": 160, "height": 44 }
    }
  ],
  "task": {
    "goal": "Implementiere die annotierten Änderungen.",
    "workspaceStrategy": "new_worktree",
    "verifyInBrowser": true
  }
}
```

**Beispiel-Prompt für den neuen T3-Thread:**

> Analysiere und bearbeite die beigefügten Review-Annotationen. Das vollständige Manifest liegt unter `<absolut erreichbarer Pfad>/batch.json`, die Screenshots relativ im dortigen `assets/`-Verzeichnis. Prüfe die DOM-Kontexte und implementiere die Änderungen im zugewiesenen Projekt-Workspace. Arbeite keine nicht dokumentierten Anforderungen hinzu. Prüfe die Änderungen anschließend im Browser, falls eine passende Browser-Session freigegeben ist, und melde Ergebnis, betroffene Dateien und offene Punkte je Annotation zurück.

**Wichtig:** Die angegebene `sessionId` ist zunächst interne Metadaten. Ein Agent darf eine Session nur dann steuern, wenn ihm dafür tatsächlich der passende MCP-Endpunkt und die Berechtigung bereitgestellt wurden.

---

## 9. Eigener MCP-Server für Agenten (P0/P1)

Die Anwendung stellt einen lokalen MCP-Server bereit, damit Coding-Agenten Review-Aufgaben und – nach Freigabe – Browser-Sessions gezielt abfragen können.

Vorgesehene Werkzeuge (konzeptuelle Namen):

| Tool | Funktion |
|---|---|
| `list_projects` | Projekte finden |
| `list_sessions` | Autorisierte Browsersitzungen und Modi auflisten |
| `get_session_context` | URL, Tab, Viewport, aktuelle Steuerungsrechte |
| `list_reviews` | Review-Sitzungen anzeigen |
| `list_annotations` | Annotationen nach Batch/Session/Status filtern |
| `get_annotation` | DOM-, Text- und Kontextdaten einer Annotation abrufen |
| `get_annotation_asset` | Screenshot-Datei bereitstellen oder lokalen Pfad liefern |
| `get_batch_manifest` | Vollständigen, versionierten Batch-Kontext abrufen |
| `update_annotation_status` | Status setzen |
| `request_browser_control` | Kontrollübernahme anfragen, an Policy gebunden |
| `release_browser_control` | Browsersteuerung freigeben |

**Browser-Automation bevorzugt mit Playwright MCP statt eigener Reimplementierung.** Der eigene Server liefert Session-Zuordnung, Review-Kontext, Berechtigung und Routing. Bei geteilten Browsern muss geklärt sein, wie Playwright MCP exakt die richtige Session anspricht. Nicht annehmen, dass ein einziger globaler MCP-Server mehrere voneinander isolierte Sessions von selbst sicher zuordnet.

---

## 10. Oberfläche und UX (P0)

Ein kompaktes, modernes Interface mit:

- **Projektleiste:** Projekt, Repository, URL, T3-Verbindung, Default-Agentprofil.
- **Session-Leiste:** Browser-Sessions, Tabs, Shared/Isolated/Observer, Control Owner.
- **Browser-/Review-Ansicht:** Browser plus Annotationsoverlay oder externes Chromium-Fenster mit Companion-Panel.
- **Live-Diktat-/Feedback-Panel:** Aufnahmezustand, aktuelle Sprache, Captures, editierbare Texte, aktiver Batch.
- **Task-Board:** Nach `Shift+N` erstellte Threads, Modell, Projekt, Worktree, Status, letzte Antwort, Fehler/Freigaben.
- **Einstellungen:** Shortcuts, Voxtype-Pfad, Datenverzeichnis, Screenshots, Redaktionsregeln, T3-Verbindung, Default-Modelle.

### Zustände im Task-Board

`draft` → `finalizing_transcript` → `queued` → `dispatching` → `running` → `completed`

Fehler-/Sonderzustände: `needs_review`, `needs_approval`, `failed`, `cancelled`, `unknown_remote_state`.

Aufgaben lassen sich unabhängig von einem erfolgreichen Ergebnis lokal abschließen (`markedDone`): auch nach Abbruch, Fehler, Rückfrage oder bei unklarem Zustand. Sie bleiben mit ihrem ursprünglichen Ausführungsstatus unter „Abgeschlossen“ erhalten und zählen nicht mehr als offener Handlungsbedarf. Die Markierung ist dauerhaft gespeichert und über „Wieder öffnen“ umkehrbar. Explizites erneutes Senden, Zuordnen oder ein Nachtrag entfernt die Markierung automatisch. Noch aktiv ausgeführte Aufgaben können zunächst abgebrochen werden.

Ein Agenten-Thread kann weiterlaufen, wenn der Benutzer eine andere Browser-Session öffnet oder einen neuen Batch aufnimmt. Die Anwendung darf den Fokus nicht unfreiwillig auf T3 Code verlagern.

---

## 11. Zusammenarbeit / mehrere Nutzer (P1)

Die erste Version darf als lokaler Einzelbenutzerbetrieb starten. Die Architektur muss die spätere Zusammenarbeit ermöglichen:

- Mehrere Nutzer erstellen Kommentare und Annotationen im selben Review.
- Live-Synchronisierung via lokalem Server/WebSocket.
- Nutzerkennung, Autorenschaft und Änderungsverlauf.
- Gemeinsame Feedback-Sammlung über mehrere Browsertabs/Sessions.
- Separate Zugriffsrechte für Sehen, Annotieren, Browsersteuerung und Task-Dispatch.
- Keine gleichzeitige unkoordinierte Steuerung desselben Tabs.
- Zugriff standardmäßig nur lokal; LAN-Freigabe explizit aktivieren, mit Authentifizierung.

Nicht im MVP verpflichtend: Internet-Hosting, Cloud-Accounts, Organisationen, Rollen-Adminoberfläche, Video-/Live-Audio-Sharing.

---

## 12. Architekturvorschlag

```text
┌────────────────────────────────────────────────────────────────────┐
│ Lokale Review-App (React/TypeScript, optional Desktop-Shell)       │
│ Projekt-/Session-UI | Annotationen | Diktat | Task-Board | Settings │
└───────────────┬────────────────────────────────────┬───────────────┘
                │                                    │
     ┌──────────▼─────────────┐         ┌────────────▼─────────────┐
     │ Chromium Browser       │         │ Lokaler Backend-Dienst   │
     │ - Tabs + Login         │◄───────►│ - Review/Batch-State     │
     │ - Content Script       │         │ - SQLite + Dateispeicher │
     │ - W/S/D Overlay        │         │ - WebSocket Events       │
     └──────────┬─────────────┘         └────────────┬─────────────┘
                │                                    │
     ┌──────────▼─────────────┐         ┌────────────▼─────────────┐
     │ Playwright MCP/CDP     │         │ Voxtype Adapter          │
     │ Human/Agent Ownership  │         │ Dateiausgabe/Segmente    │
     └──────────┬─────────────┘         └────────────┬─────────────┘
                │                                    │
                └─────────────────┬──────────────────┘
                                  │
                       ┌──────────▼──────────┐
                       │ Dispatch-Queue      │
                       │ Manifest + Assets   │
                       │ Idempotenz/Retry    │
                       └──────────┬──────────┘
                                  │
                       ┌──────────▼──────────┐
                       │ T3-Code-Adapter     │
                       │ Modell/Thread/State │
                       └──────────┬──────────┘
                                  │
                       ┌──────────▼──────────┐
                       │ Bestehende T3 Code  │
                       │ Provider + Agenten  │
                       │ optional Worktrees  │
                       └─────────────────────┘
```

### Technologiepräferenz

- TypeScript für UI, Orchestrierung, Adapter und MCP-Server.
- React + Vite für Review-Oberfläche.
- Node.js/Bun als lokaler Service; Auswahl konsistent treffen.
- Chromium + Playwright/CDP für Browserautomatisierung.
- Chromium-Extension/Content-Script für exaktes DOM-Element-Picking und Overlay.
- SQLite für lokale persistente Metadaten; Dateisystem für PNG/WebP, JSON und Transkripte.
- WebSocket für lokale Status-/Transcript-Events.
- Desktop-Shell optional: Tauri oder Electron nur, **wenn** echte Systemintegration (Tray, globale Hotkeys, Fenstersteuerung) es erfordert. Keine unnötige doppelte Browser-Engine.
- KDE/Wayland: globale Shortcuts durch systemunterstützte Mechanismen; Screenshots bevorzugt innerhalb des verwalteten Browsers. Nicht von universellem Input-Sniffing oder uneingeschränkter Desktop-Capture ausgehen.

### Datenhaltung (Mindestentitäten)

`Project`, `BrowserSession`, `BrowserTab`, `Review`, `Batch`, `Annotation`, `TranscriptSegment`, `Asset`, `AgentProfile`, `T3Task`, `TaskEvent`, `Participant`.

**Stabile UUIDs**, timestamps, schema migration, status history und Referenzen zu Assets verwenden. Sensible Daten nicht mit Annotationen in Klartext ablegen.

### Beispielverzeichnis

```text
~/.local/share/browser-review/
  app.sqlite
  projects/<project-id>/
    reviews/<review-id>/
      batches/<batch-id>/
        batch.json
        assets/
          ann-001.png
          ann-001-original.png
        transcripts/
          segment-001.txt
  sessions/<session-id>/
    profile/   # nur falls Browserprofil lokal von der App verwaltet wird
```

---

## 13. Sicherheit und Zuverlässigkeit

1. Lokale Backend-, MCP- und CDP-Endpunkte standardmäßig nur über `127.0.0.1` bzw. sichere lokale IPC verfügbar machen.
2. Lokale Clients authentifizieren bzw. Zugriffe über restriktive Dateirechte/Token begrenzen; keine ungeschützten Debug-Ports ins LAN.
3. Browserprofile, Cookies und Speicherzustände als sensibel behandeln; keine Speicherung im Manifest.
4. Screenshots/DOM können Kundendaten enthalten: konfigurierbare Redaktion, explizite Freigabe und Sichtprüfung ermöglichen.
5. Lokale Transkription bedeutet **nicht**, dass an T3 gesendete Prompts/Bilder lokal verarbeitet werden. Provider-/Datenfluss transparent anzeigen.
6. Maximal erlaubte Berechtigungen pro Agent-/MCP-Session festlegen; menschliches Takeover jederzeit möglich.
7. Kein automatisches Klicken gefährlicher Aktionen ohne Freigabe: Löschungen, Zahlungen, externe Veröffentlichungen, irreversible Datenänderungen.
8. Bei Dispatch-Retry Thread-Duplikate vermeiden; bei Zeitüberschreitungen erst Status/relevante Threads prüfen.
9. Bei Absturz: offenen Batch, nicht versandte Captures und T3-Zuordnungen wiederherstellen.
10. Keine direkte Manipulation der internen T3-Datenbank, keine Cookie-Exporte in KI-Prompts.
11. Agenten mit parallelen Schreibrechten standardmäßig getrennte Worktrees zuweisen.
12. Sichere lokale Pfade, Dateinamen und Shell-Argumente: keine unkontrollierte Shell-Interpolation aus Webseiteninhalt.

---

## 14. Prioritäten und Entwicklungsphasen

### Phase 0 – Technische Machbarkeitsprüfung (zwingend vor der eigentlichen Implementierung)

- Betriebssystem, KDE-/Wayland-/X11-Umgebung, Browser und Shortcuts prüfen.
- `t3 --help`, laufende T3-Instanz, Authentifizierung, tatsächlich verfügbare externe API/MCP-Funktionen und Modelle ermitteln.
- **Echten Test-Thread extern erstellen**, Modell setzen und Status empfangen – ohne T3 zu modifizieren.
- `voxtype --help` und Dateiausgabe testen.
- Browser-PoC: Benutzer loggt sich ein; KI bedient nachfolgend **denselben** sichtbaren Tab; Benutzer übernimmt die Kontrolle.
- DOM-Picker und Screenshot im echten Browser testen.
- Ergebnisse/Blocker in `docs/feasibility.md` dokumentieren; jede Annahme nachprüfbar machen.

### Phase 1 – Funktionaler MVP (P0)

- Projekt- und Browser-Session-Verwaltung (Shared + Isolated).
- Review-Modus mit `W`, `S`, `D`.
- Voxtype: zuverlässige einfache Sprachannotation, file-based.
- Persistenter Batch mit Annotationen und editierbaren Transkripten.
- `Shift+N`: Snapshot, sofortiger neuer Batch, asynchroner T3-Thread.
- Provider/Modell in der eigenen App wählen, soweit T3 verfügbar.
- T3-Status und Fehler in eigenem Task-Board.
- Getrennte Worktrees/Schreibkonfliktwarnung für Parallelität.
- Lokale Datenhaltung und Neustart-Wiederherstellung.

### Phase 2 – Verbesserter Sprach-/Agentenworkflow (P1)

- Kontinuierliche Aufnahme und saubere sprachliche Segmentierung.
- Agentensteuerungs-Pause, Observer-Modus und visuelle Statusanzeige.
- Follow-ups an bestehende Threads, Modell-Presets.
- Erweiterte Zeichentools, automatisches DOM-Relinking.
- Gezieltes Wiederöffnen von Seiten/Annotationen.
- Feedback-Status-Synchronisation mit T3, soweit möglich.

### Phase 3 – Zusammenarbeit und Komfort (P2)

- Gemeinsame Reviews mit mehreren Personen.
- LAN-Host-Modus mit Nutzer-/Rechteverwaltung.
- Kollaborative Annotationsbearbeitung und Live-Updates.
- Umfangreiche Suche, Export/Import, projektübergreifende Vorlagen.

---

## 15. Konkrete Abnahmetests

| ID | Testfall | Erwartung |
|---|---|---|
| AT-01 | Login in verwaltetem Browser, Neustart | Session bleibt bei persistentem Profil angemeldet, sofern Website dies zulässt |
| AT-02 | Shared Session | KI klickt in **demselben sichtbaren Tab**, den der Benutzer gerade betrachtet |
| AT-03 | Takeover | Benutzer pausiert Agent; keine weitere Agenteninteraktion im Tab bis zur Freigabe |
| AT-04 | Isolated Session | Zwei Browser-Kontexte haben keine unbeabsichtigt gemeinsamen Cookies/Logins |
| AT-05 | `W` | Screenshot mit korrekt platziertem Ring um Cursor, URL und Zeitstempel |
| AT-06 | `S` | Gewählter Ausschnitt stimmt bei Browser-Zoom und HiDPI mit sichtbarer Auswahl überein |
| AT-07 | `D` | Korrektes Element erhält robusten Locator, Elementdaten und Screenshot |
| AT-08 | Sprache | Voxtype-Datei wird vollständig gelesen und eindeutig zur Annotation gespeichert |
| AT-09 | Mehrere Marker | Drei Captures mit unterschiedlichem Sprachtext ergeben getrennte korrigierbare Einträge |
| AT-10 | `Shift+N` | Alter Batch wird abgeschlossen; neuer Batch ist sofort bedienbar, während T3 arbeitet |
| AT-11 | Zweiter Dispatch | Zwei T3-Threads laufen gleichzeitig und beide sind im Task-Board sichtbar |
| AT-12 | Modellauswahl | T3-Thread startet mit tatsächlich ausgewähltem unterstütztem Provider/Modell |
| AT-13 | Task-Kontext | Agent kann alle referenzierten Screenshots, Transkripte, DOM-Daten und URLs lesen |
| AT-14 | Transkriptverzögerung | Verzögerte Transkription landet im **richtigen alten Batch**, nicht im neuen |
| AT-15 | Fehler/Retry | Verbindungsfehler verliert keinen Batch und erstellt beim Retry nicht still doppelte Threads |
| AT-16 | Parallelität | Zwei schreibende Threads verändern keine versehentlich gemeinsam genutzte Working Copy |
| AT-17 | Texteingabe | `W`, `S`, `D` schreiben normal in Eingabefelder, wenn Review-Kurzbefehle dort deaktiviert sind |
| AT-18 | Neustart | Projekt, Batch, Annotationen, Assets und bekannte T3-Thread-IDs bleiben erhalten |
| AT-19 | Sicherheit | Browser-Cookies/Passwörter tauchen weder im Batch-Manifest noch im Agenten-Prompt auf |
| AT-20 | Kein T3-Patch | Alle Funktionen laufen, ohne T3-Code-Repository oder dessen Datenbank zu verändern |

---

## 16. Explizite Nicht-Ziele für das MVP

- Kein Neuschreiben von T3 Code oder eines Coding-Agenten.
- Keine neue STT-Engine entwickeln, solange Voxtype zuverlässig integrierbar ist.
- Keine eigene Rendering-Engine oder eigener Browser-Stack.
- Kein verpflichtender Cloud-Account und kein Hosting.
- Keine automatische Code-Merge-/Deploy-Funktion.
- Keine Behauptung, ein bestimmter T3-Thread-CLI-Befehl existiere, bevor er lokal verifiziert wurde.
- Keine garantierte unterbrechungsfreie kontinuierliche Diktierung mit Voxtype, **ohne** die technischen Möglichkeiten der lokal installierten Version zu prüfen.

---

## 17. Offene Entscheidungen (mit empfohlenen Defaults)

| Frage | Default für den ersten Build |
|---|---|
| Browser | Eigene verwaltete Chromium-Instanz statt beliebigem Systembrowser |
| Interface | React-Companion-UI + Browser-Extension/Content-Script |
| Browserautomation | Playwright MCP / CDP mit gezielter Session-Bindung |
| Sessions | Shared und Isolated im MVP, Observer danach |
| Shortcuts | `W`, `S`, `D` nur im Review-Modus; `Shift+N` zum Dispatch |
| Sprache | Explizite Segmentierung zuerst; kontinuierliche Aufnahme danach |
| Agentenrouting | Ein neuer T3-Thread je abgeschlossenem Batch |
| Parallelität | Neuer Git-Worktree pro gleichzeitig schreibendem Task |
| Model Selection | Tatsächliche T3-Modelle dynamisch abfragen |
| Zusammenarbeit | Single-User MVP, Multi-User später |
| Persistenz | SQLite + lokale Asset-Dateien |

---

## 18. Arbeitsauftrag an den implementierenden Coding-Agenten

**Implementiere eine lauffähige Version dieses Produkts. Arbeite inkrementell und beginne mit einem belastbaren End-to-End-Spike.**

1. Lies dieses Dokument und prüfe die installierte Umgebung, statt APIs oder CLI-Befehle zu erfinden.
2. Erstelle `docs/feasibility.md` mit der konkret verfügbaren T3-Code- und Voxtype-Anbindung, getesteten Versionen und bekannten Einschränkungen.
3. Schlage eine minimale Architektur vor und entscheide, welche Teile als Extension, lokaler Service und UI laufen.
4. Implementiere zuerst **Browser Shared/Isolated + manuelles Login + Agentensteuerung + Takeover**.
5. Implementiere danach `W`, `S`, `D` einschließlich stabiler Dateispeicherung und DOM-Kontext.
6. Integriere Voxtype mit **korrekter lokaler CLI** und eindeutiger Zuordnung zu Captures.
7. Implementiere den **Batch-State-Machine-Workflow** mit `Shift+N`, asynchronem Dispatch, unmittelbarem neuen Batch, Retry/Idempotenz und T3-Thread-Tracking.
8. Erlaube dynamische Wahl von Provider/Modell und sichere parallele Worktree-Ausführung, soweit die tatsächliche T3-Schnittstelle es zulässt.
9. Erstelle automatisierte Tests für State Machine, Routing, Transkriptgrenzen und Dateireferenzen sowie einen echten Browser-Integrationstest.
10. Gib einen reproduzierbaren Linux-Installations- und Startprozess, eine `.env.example` ohne Secrets und eine knappe Bedienungsanleitung aus.

**Lieferkriterien:** funktionierender End-to-End-Durchstich, dokumentierte Versionen und Limitationen, nachvollziehbare Architektur, Tests, und eine Demo gemäß AT-01 bis AT-20 (P0-Abnahmekriterien priorisiert).

**MVP-Demo in einem Satz:** *Ich öffne meine Web-App, markiere drei Stellen mit `W`/`S`/`D`, spreche Änderungen dazu ein, drücke `Shift+N` und sehe Agent A in T3 arbeiten, während ich schon neue Markierungen für Agent B aufnehme – ohne T3 Code zu verändern.*

---

## 19. Technische Quellen zum Verifizieren (Stand: 10.11.2026)

Die folgende Dokumentation dient als **Prüfgrundlage**; die lokal installierte Version und ihre tatsächlich verfügbaren Funktionen sind maßgeblich:

- T3 Code, offizielles Projekt: https://github.com/pingdotgg/t3code
- T3 Code – Installation und CLI-Grundlagen: https://github.com/pingdotgg/t3code/blob/main/docs/user/install.md
- T3 Code – Threads und Worktrees: https://github.com/pingdotgg/t3code/blob/main/docs/user/thread-sidebar.md
- T3 Code – Orchestrator-MCP-Dokumentation: https://github.com/pingdotgg/t3code/blob/main/docs/orchestration-v2/orchestrator-mcp-server.md
- Voxtype – Bedienungsanleitung (`record start --file=...`): https://github.com/peteonrails/voxtype/blob/dev/docs/USER_MANUAL.md
- Playwright MCP – Browser-/Extension-Verbindung: https://playwright.dev/mcp/configuration/browser-extension
- Playwright MCP – Browserprofile und Sessions: https://playwright.dev/mcp/configuration/user-profile
