# Kubri — Design- und UX-Review

Stand: 29. Oktober 2026. Empfehlung: Kubri als konzentrierten Kubernetes-Arbeitsplatz weiterentwickeln — mit sichtbarem Kontext, priorisierten Problemen und einem zusammenhängenden Diagnoseablauf.

**Nachfolgende Entscheidung: 02 / Link ist das gewählte Branding.** Diese Nutzerentscheidung ersetzt die ursprüngliche Span-Empfehlung. Der Prototyp und die aktuellen Screenshots verwenden nun die Link-Marke und ihre blaue Farbwelt. Die konkrete Umsetzung ist im [Umsetzungsplan](IMPLEMENTATION-PLAN.md) beschrieben.

[Klickbaren Prototyp öffnen](index.html) · [Logo-Konzepte](index.html#brand) · [KI-Exploration](logo-explorations.png)

## Grundlage und Grenzen

Geprüft wurden README, die vorhandenen Produkt-Screenshots, die Svelte-Oberfläche und die zugehörige Zustands- und Interaktionslogik. Schwerpunkt: Navigation, Ressourcenlisten, Details/YAML, Logs, Pulses, Kontextwechsel und Aktionsdialoge. Die vorhandenen Screenshots zeigen teilweise einen älteren Stand: Im aktuellen Code sind beispielsweise bereits eine ActionBar und ein sichtbarer „Faults“-Filter vorhanden.

Die Befunde unten stammen aus der statischen Codeprüfung und der Sichtprüfung der vorhandenen Screenshots. Die bestehende Anwendung wurde nicht mit einem Kubernetes-Cluster ausgeführt. Dies ist kein vollständiges Backend-, Sicherheits- oder Performance-Audit und keine Nutzerstudie. Neue Verhaltensweisen im Prototyp sind Vorschläge, keine bereits implementierten Produktfunktionen.

Die produktive Anwendung bleibt unverändert. Alle neuen Dateien liegen unter `docs/design-review/`. Sämtliche Cluster, Messwerte, Fehler, Logs und Verbindungen im Prototyp sind synthetische Beispieldaten.

## Was erhalten bleiben sollte

- Die Kombination aus Mausbedienung und k9s-Tasten ist ein klarer Produktvorteil. Die Command Palette, sichtbare Aktionen und Kontextmenüs bereits im aktuellen Code sind eine gute Grundlage.
- Rust-Core, Tauri- und Web-Zugang teilen sich dieselbe UI. Ein konsistentes Designsystem kann dadurch beide Nutzungsformen verbessern.
- Live-Ressourcen, Logs mehrerer Container, vorherige Containerinstanzen, YAML, XRay, RBAC und Helm bilden schon einen anspruchsvollen Arbeitsablauf ab. Das Redesign muss diese Tiefe zugänglich machen.
- Helles/dunkles Erscheinungsbild, k9s-Skins, Fehlerhinweise, Read-only-Modus und Bestätigungsdialoge existieren bereits. Der Vorschlag baut darauf auf.

## Priorisierte Befunde

P1 = zuerst beheben, weil Nutzer unbeabsichtigt Arbeit verlieren oder unerwartete Aktionen auslösen können. P2 = erhebliche Klarheits- oder Bedienverbesserung. P3 = nachfolgende Verfeinerung.

| Priorität | Beobachtung und Beleg | Auswirkung | Empfehlung |
| --- | --- | --- | --- |
| **P1** | „Delete finished pods…“ verwendet einen Filter, der auch `Pending`, `CrashLoopBackOff`, `ImagePullBackOff` und weitere Fehlerzustände einschließt. [ResourceView.svelte](../../ui/src/lib/components/ResourceView.svelte), Zeilen 286–294 und 322. | Die Beschriftung kündigt eine engere Auswahl an, als anschließend im Löschdialog landet. Wartende oder fehlerhafte Pods sind nicht zwangsläufig abgeschlossene Pods. Ein Bestätigungsdialog existiert, beseitigt aber die unpräzise Vorauswahl nicht. | „Abgeschlossene Pods löschen“ auf wirklich abgeschlossene Zustände beschränken. „Fehlerhafte Pods bereinigen“ separat anbieten. Vor dem Bestätigen vollständige Auswahl, Zustand, Kontext und Namespace anzeigen. |
| **P1** | `DetailsPanel.close()` entfernt das Panel direkt. Der Cancel-Button ruft `cancel()` ohne Änderungsflag auf; nur der Editor-Escape-Pfad übergibt den Vergleich zum ursprünglichen Text. [DetailsPanel.svelte](../../ui/src/lib/components/DetailsPanel.svelte), Zeilen 93–96, 126–138 und 189; [YamlEditor.svelte](../../ui/src/lib/components/YamlEditor.svelte), Funktion `escape`. | Ungespeicherte YAML-Änderungen können über Schließen oder Cancel ohne dieselbe Warnung wie bei Escape verloren gehen. | Ein zentraler Änderungsstatus und ein gemeinsamer Verlassen-Dialog für Schließen, Cancel, Objekt-/Kontextwechsel und Tabwechsel. Optionen: Weiter bearbeiten, verwerfen, speichern. |
| **P2** | Tabellenfehler und Leermeldung werden unabhängig gerendert. Bei einem fehlgeschlagenen ersten Ladevorgang setzt `LiveTable` loading auf false; eine leere Liste kann zusätzlich „No … objects“ anzeigen. [ResourceView.svelte](../../ui/src/lib/components/ResourceView.svelte), Zeilen 540 und 620–630; [watch.svelte.ts](../../ui/src/lib/watch.svelte.ts), Zeilen 33–35. | „Nicht vorhanden“ und „nicht abrufbar“ vermitteln unterschiedliche Tatsachen. Nutzer können eine Berechtigungs- oder Verbindungsstörung als leeren Namespace interpretieren. | Getrennte Zustände: lädt, leer, kein Filtertreffer, Zugriff verweigert, Verbindung unterbrochen. Vorhandene Daten bei Unterbrechung sichtbar lassen und als veraltet kennzeichnen. Mockup: Pods → Preview state. |
| **P2** | Ein Fehler beim Metrics-Polling setzt `available=false`; Ressourcenlisten fügen die Metrikspalten nur bei `available` ein. [watch.svelte.ts](../../ui/src/lib/watch.svelte.ts), Zeilen 49–60; [ResourceView.svelte](../../ui/src/lib/components/ResourceView.svelte), Zeilen 95–98. | Bei vorübergehenden Fehlern verschwinden Spalten; fehlende Daten, fehlender metrics-server und fehlende Berechtigungen bleiben schwer unterscheidbar. | Spalten stabil halten; „nicht verfügbar“, Zeitpunkt der letzten Messung und Fehlergrund darstellen. Niemals fehlende Werte als null Verbrauch ausgeben. |
| **P2** | Das Hauptlayout reserviert 240 px für die Sidebar und mindestens 380 px bzw. 44 % für Details; der Kontextschalter hat mindestens 260 px Breite. [Seitenlayout](../../ui/src/routes/+page.svelte), Zeilen 463–467; [ContextSwitcher.svelte](../../ui/src/lib/components/ContextSwitcher.svelte), Zeile 95. | Auf kleineren Fenstern wird die Tabelle stark zusammengedrückt; die breite Kopfzeile konkurriert mit der Arbeitsebene. | Auf großen Fenstern ein begrenztes, idealerweise verstellbares Detailpanel. Unterhalb einer sinnvollen Breite eine eigene Detailansicht mit Rückkehr zur unveränderten Liste. Im Mockup erfolgt dieser Wechsel bei 1000 px. |
| **P2** | „Tools“ mit acht Einträgen steht vor einer langen gemischten Ressourcenliste. Pulses zeigt Ressourcenkacheln vor den aktuellen Warnungen. [Sidebar.svelte](../../ui/src/lib/components/Sidebar.svelte), Zeilen 43–71; [PulsesView.svelte](../../ui/src/lib/components/PulsesView.svelte), Zeilen 177–184. | Häufige Tätigkeiten und dringende Probleme erhalten wenig visuelle Priorität. Viele gesunde bzw. leere Ressourcentypen ziehen ähnlich viel Aufmerksamkeit wie Störungen auf sich. | Übersicht + Workloads + Cluster + Explore gruppieren. Probleme vor die vollständige Inventarliste stellen; vorhandene Namen als Aliase erhalten, z. B. „Overview / :pulses“ und „Topology / XRay“. |
| **P2** | Löschdialoge zeigen Objektnamen inklusive Namespace, aber keinen eigenen Clusterkontext. Bei entsprechenden Dialogtypen erhält die Bestätigung initial den Fokus. [ActionDialog.svelte](../../ui/src/lib/components/ActionDialog.svelte), Zeilen 142, 308–315 und 409–417. | Kontextwechsel und schnelle Tastaturfolgen verdienen bei produktiven Schreibaktionen besondere Klarheit. Die bestehende Bestätigung sollte das Ziel vollständig benennen. | Kontext, Namespace, Zielanzahl und Folgen direkt im Dialog. Bei destruktiven Aktionen initial Cancel fokussieren; zusätzliche Bestätigung gezielt für Produktions-/Massenaktionen einsetzen. Read-only sichtbar halten. |
| **P2** | `%CPU/R` und `%CPU/L` sind technisch präzise, verwenden aber denselben Schwellwertmechanismus; „Request“ und „Limit“ sind visuell wenig erklärt. [ResourceView.svelte](../../ui/src/lib/components/ResourceView.svelte), Metrikdefinitionen ab Zeile 67 und Funktion `tone`. | Ein Wert über 100 % des Requests kann als Überschreitung eines harten Limits verstanden werden. | „CPU used / request“ und „CPU used / limit“ ausschreiben oder eindeutig erklären. Absolutwert + Bezugswert zusammen zeigen. Der Mockup-Fall „200m / 125m = 160 %“ benennt ausdrücklich den Request. |
| **P3** | Sortierbare Tabellenüberschriften verwenden `onclick` auf `th`; Checkboxen und Zeilenmenüs sind teilweise aus der Tab-Reihenfolge genommen. [ResourceView.svelte](../../ui/src/lib/components/ResourceView.svelte), Zeilen 560, 584 und 605. | Die vorhandenen globalen k9s-Kürzel ersetzen keine gut auffindbaren semantischen Bedienelemente für Tastatur- und Screenreader-Nutzung. | Native Buttons in den Überschriften, `aria-sort`, benannte Auswahlen, klarer Fokus und dokumentierte Tabellen-Navigation. Vorhandene Kürzel zusätzlich erhalten. |

## Die Mockups

### 01 — Cluster overview

[Vorschau](screenshots/01-overview-light.png)

Eine kompakte Kennzahlenzeile beantwortet „Ist der Cluster grundsätzlich gesund?“. Direkt darunter folgen priorisierte Probleme mit konkreten nächsten Schritten. Ressourcengesundheit und CPU-Verlauf stehen ergänzend daneben. Ein Fehlzustand führt direkt zum betroffenen Pod.

Die CPU-Historie bleibt als Sitzungsverlauf gekennzeichnet. Der Entwurf verspricht keine mehrtägigen Metriken, die Kubri aktuell nicht speichert. Clusterweite Messwerte sind auch bei gesetztem Namespace ausdrücklich als clusterweit beschriftet.

### 02 — Pods und Inspektor

[Desktop](screenshots/02-pods-inspector.png) · [Schmale Ansicht](screenshots/mobile-pods.png)

Filter nach Name/Label, sichtbare Statusfilter, klare Auswahlanzeige und ein Inspektor mit Status, Ursachehinweisen, Fakten und Ereignissen. Bei CrashLoopBackOff ist „Read previous container logs“ die Hauptaktion. Das nutzt eine bestehende Fähigkeit und verkürzt ihren Zugangsweg.

Auf schmalen Ansichten ersetzt der Inspektor vorübergehend die Liste. Schließen stellt die Liste mit ihren Filtern wieder her. Der Prototyp bietet außerdem Verbindungsabbruch, Zugriff verweigert und leere Filterergebnisse zum Durchspielen.

### 03 — Container logs

[Dunkle Vorschau](screenshots/03-logs-dark.png)

Kontext, Namespace, Pod und Container bleiben sichtbar. „Previous · exited“ ist explizit von „Current · waiting“ getrennt. Der Hinweis auf eine beendete Instanz verhindert die Erwartung eines weiterlaufenden Streams. Suche, beispielhafte Level-Filter und Export sind bedienbar.

„First clue“ beschreibt einen manuell vorbereiteten Demonstrationsfall. Es ist keine automatische Ursachenanalyse. Level-Filter wären eine Ergänzung: Echte Containerlogs sind nicht immer strukturiert und benötigen passende Parser oder einen unstrukturierten Fallback.

### 04 — Connect

[Vorschau](screenshots/04-connect.png)

Kontext wählen, Umgebung erkennen und vor dem Einstieg einen Read-only-Modus festlegen. Der Fehlerzustand nennt einen konkreten nächsten Schritt. Für die echte Anwendung sollen Produktionskennzeichen konfigurierbar sein; der Kontextname allein darf keine verlässliche Sicherheitsklassifizierung vortäuschen. UI-Read-only ersetzt keine Kubernetes-Berechtigungen.

### Zusätzliche Zustände

[Löschbestätigung](screenshots/05-delete-confirmation.png) · [Zugriff verweigert](screenshots/06-access-denied.png)

Der Löschdialog zeigt das vollständige Ziel und die Umgebung. „Simulate deletion“ verändert keine Beispieldaten und keinen Cluster. Die YAML-Ansicht ist im Mockup bewusst nur lesbar; ein neuer Editor mit Änderungsprüfung ist noch nicht implementiert.

## Logo-Richtungen

[Editierbare Konzeptübersicht](screenshots/07-logo-concepts.png) · [KI-Explorationsboard](logo-explorations.png)

| Entwurf | Charakter | Einschätzung |
| --- | --- | --- |
| **Span** | Reduzierter Brückenbogen, Petrol, kleine Mintfuge | Frühere Alternative; wird nicht als Branding weiterverfolgt. |
| **Link** | Geometrisches K, klares Blau, kantige Form | **Vom Nutzer gewählt.** Verbindliche Markenrichtung für den Umsetzungsplan. |
| **Portal** | Rechteckiger Zugang, Schwarz und Kupfer | Ruhige Alternative mit kompakter Silhouette. |

Die SVGs sind eigenständige, geometrische Ausarbeitungen der Richtungen; sie sind keine exakten Vektorisierungen des KI-Bildes. Das KI-Board dient der Stil- und Formexploration. Das Wortlogo im Prototyp nutzt Systemschrift und ist noch kein ausgearbeitetes finales Lettering. Vor einer Übernahme folgen optische Korrekturen, echte monochrome/reverse Varianten, 16-/24-/32-px-Tests und Export als Tauri-App-Icons. Die gegenwärtigen App-Icons wurden nicht ersetzt.

## Umsetzung in sinnvollen Schritten

1. **Verlässliche Aktionen und Zustände:** Bereinigungsfilter, zentrale Warnung bei ungespeichertem YAML, eindeutige Fehler-/Leermeldungen, stabile Metrikspalten. Gezielte Verhaltenstests für diese Fälle ergänzen.
2. **Gemeinsames UI-Fundament:** semantische Farbvariablen, Typografie, Abstände, Fokusdarstellung, Kontext-/Namespace-Leiste und responsive Master-Detail-Ansicht. Bestehende k9s-Skins und Dark Mode berücksichtigen.
3. **Diagnoseablauf:** priorisierte Übersicht, direkter Weg zum betroffenen Objekt, aufbereitete Statuszusammenfassung und Ereignisse; gezielter Einstieg in vorherige Containerlogs.
4. **Marke und Feinschliff:** Das ausgewählte Link-Logo finalisieren und in allen Größen prüfen. Dann die zusätzlichen Bereiche Helm, RBAC, XRay, Sanitizer, Port-forwards und Benchmarks an das gemeinsame Muster anpassen. Die vollständige Reihenfolge steht im [Umsetzungsplan](IMPLEMENTATION-PLAN.md).

Mit Nutzern prüfen: Finden sie den fehlerhaften Pod? Erreichen sie die richtige Loginstanz? Erkennen sie vor einer Aktion den Zielcluster? Bleibt die k9s-Bedienung gleich schnell? Geeignete Messgrößen sind benötigte Schritte, Fehlklicks und Zeit bis zur relevanten Information; es wurden noch keine Verbesserungsprozente erhoben.

## Verifikation der gelieferten Artefakte

Der eigenständige Prototyp wurde lokal in Headless Edge ausgeführt. Geprüft: Pod-Auswahl, Text-/Labelfilter, leere Treffer, Problemfilter, YAML-/Ereignisansicht, Loginstanzen, Level-/Textfilter, Command Palette, Kontextwechsel, Verbindungsfehler, Read-only, Löschdialog, veraltete Daten, Zugriff verweigert und Logowechsel.

Für fünf Ansichten wurden die Dokumentbreiten bei 1024, 768, 390 und 320 px geprüft: 20 Kombinationen ohne horizontalen Dokumentüberlauf. Tabellen und Logs dürfen innerhalb ihrer eigenen Bereiche scrollen. Desktop- und mobile Screenshots wurden visuell kontrolliert. Keine JavaScript-Laufzeitfehler in diesen Prüfungen. Dies ist keine vollständige Accessibility-Zertifizierung und kein Test der produktiven Anwendung.

[Prüfskript](check-mockups.cjs) · [Prüfergebnis](verification.json) · [Bildgenerierungs-Prompt](IMAGEGEN-PROMPT.md)
