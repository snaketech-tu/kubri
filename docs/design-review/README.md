# Kubri Design Study

**Festgelegtes Branding: 02 / Link.** Der Prototyp verwendet standardmäßig die K-Marke und eine blaue Farbwelt. Span und Portal bleiben als frühere Alternativen verfügbar.

[Umsetzungsplan mit Reihenfolge und Abnahmekriterien](IMPLEMENTATION-PLAN.md)

Die Datei [index.html](index.html) direkt im Browser öffnen. Der Prototyp benötigt keinen Build, keine Installation, keine Internetverbindung und keinen Kubernetes-Zugang.

- **01 Overview:** Probleme erkennen und zum betroffenen Pod springen.
- **02 Pods:** Filtern, sortieren, Details/Ereignisse/YAML ansehen; unter „Preview state“ Fehlerzustände ausprobieren.
- **03 Logs:** Vorherige/aktuelle Instanz, Level und Text filtern; sichtbare Demo-Logs exportieren.
- **04 Connect:** Einen Demo-Kontext auswählen; Verbindungsfehler ausprobieren.
- **05 Logos:** Drei SVG-Konzepte vergleichen und in die Prototyp-Kopfzeile übernehmen.

Der Mond-/Sonnenbutton wechselt Light/Dark Mode. `Ctrl+K` oder `:` öffnet die Command Palette, `/` fokussiert den Filter, `l` öffnet das Logbeispiel, `?` zeigt Hilfe. Der Read-only-Button ermöglicht die Vorschau des Löschdialogs; auch dort werden keine echten Aktionen ausgeführt.

[Vollständiger Review mit Quellcodebelegen](REVIEW.md)

Alle Daten sind synthetisch. Ausgegraute Navigationseinträge markieren Bereiche außerhalb dieses Prototyps. Die drei SVGs sind geometrische Entwürfe; das separate PNG ist eine KI-generierte Stilstudie. Der Umsetzungsstand in der Anwendung steht im [Umsetzungsplan](IMPLEMENTATION-PLAN.md#status-at-handoff); Screenshots der Anwendung mit Testdaten liegen in `application/`.

## Dateien

- `index.html`, `mockups.css`, `mockups.js`: eigenständiger klickbarer Prototyp.
- `logos/`: drei editierbare SVG-Entwürfe.
- `logo-explorations.png`: generiertes Markenboard.
- `screenshots/`: Desktop-, Dialog- und Mobilansichten als PNG.
- `REVIEW.md`: priorisierte Befunde, Designentscheidungen, Umsetzungsschritte und Grenzen.
- `IMPLEMENTATION-PLAN.md`: beschlossene Link-Markenrichtung, geplante Changesets und Abnahmekriterien.
- `IMAGEGEN-PROMPT.md`: vollständiger Prompt und Herkunft des Markenboards.
- `check-mockups.cjs`, `verification.json`: reproduzierbare Interaktions-/Layoutprüfung und Ergebnis.

## Prüfung wiederholen

Mit Node.js und einer vorhandenen Playwright-Installation:

```sh
node check-mockups.cjs /absolute/path/to/playwright /absolute/path/to/browser
```

Ohne Argumente wird ein lokal auflösbares `playwright` und dessen Standard-Chromium verwendet. Die Prüfung öffnet die lokale HTML-Datei in einem temporären Browserprofil und schreibt Screenshots sowie `verification.json` in diesen Ordner.
