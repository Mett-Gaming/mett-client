# Mett-Loader (Vorbild SKSE)

| SKSE | Mett |
|---|---|
| `skse64_loader.exe` | `mett-inject`, später aufgerufen von `MettClient.exe --launcher` |
| Kern-DLL | `mett_core.dll`, ohne Hooks und Threads |
| `Data/SKSE/Plugins/*.dll` | `plugins/*.dll`, nur namentlich eingeschaltete |
| `Query` und `Load` | `mett_plugin_query`, `mett_plugin_load`, zusätzlich `mett_plugin_unload` |
| Address Library | Suche über Namen und Merkmale im Plugin, keine festen Adressen |

## Ablauf
1. Kern laden (`LoadLibrary`), dann `mett_core_start` mit den eingeschalteten Namen aufrufen. In `DllMain` passiert nichts.
2. Alle Plugins fragen (`query`, darf nichts verändern), dann nur die passenden laden.
3. Stand je Plugin über gemeinsamen Speicher `MettCore.<PID>` lesen, für die Anzeige im Menü.
4. Stopp: Plugins in umgekehrter Reihenfolge zurücknehmen, Kern entfernen. Scheitert das, bleibt der Kern geladen.

## Unterschiede zu SKSE
- SKSE startet das Spiel selbst angehalten; Bedrock startet über die App-Kennung, geladen wird danach.
- Keine festen Versionen: Jedes Plugin entscheidet in `query`, ob es zur Spielversion passt.
- Nichts wird in den Spielordner geschrieben, Spieldateien bleiben unverändert.

## Geprüft (25.11.2026, nur an ping.exe)
Gutes Plugin läuft; Schrott, fremde DLL, fehlende Datei, Pfad-Trick und Ablehnung fallen einzeln aus; zweiter Start lädt nicht doppelt; nach dem Stopp sind Kern, Plugin und gemeinsamer Speicher weg; Testprozess lebt.

## Grenzen
- Stürzt ein Plugin hart ab (Zugriffsfehler), stürzt das Spiel mit ab. Getrennt sind nur Laden und Prüfen.
- In Minecraft noch nie geladen. Es bleibt Injection, mit Bannrisiko auf Servern.

## Namen statt Adressen
`compat::image` sucht im geladenen Abbild: Klassenname → Typeintrag → Funktionstabelle, sowie Knopfkennungen (FNV-1a) und Funktionsgrenzen. `compat::chat_send` ist die erste Kette darauf; sie nennt bei einem Fehlschlag die Stufe.
Grenzen: Nicht jede Klasse hat einen lesbaren Namen (`ChatScreenController` und `GuiData` nur indirekt). Funktionsnamen gibt es kaum, ab dem Einstieg zählt die Form des Codes. Aufbau von Objekten (Felder) ändert sich unabhängig von Namen und wird bei jedem Zugriff geprüft.
Prüfen am laufenden Spiel: `cargo run --release -p gamemem --example namen` (nur lesen).

## Erster Versuch in Minecraft (25.11.2026, Hauptmenü, keine Welt offen)
Zweimal versucht (normal und ohne Zusatzdateien gebaut): Der Kern wurde nicht geladen, `LoadLibrary` brachte kein Modul; Minecraft lief unverändert weiter, kein Absturz. Danach wie angekündigt abgebrochen.
Laut Windows keine Signatursperre und kein Verbot fremder DLLs (Discords Hook ist geladen). Ursache offen; `LoadLibrary` meldet den genauen Fehler bisher nicht zurück. Im Testprozess geht derselbe Kern.
`minecraft://?load=` und `connect/?localLevelId=` öffneten die Debug-Welt nicht. Angelegt: Welt „Mett-Debug“ (Ordner `MettDebug1`, Kopie, Mehrspieler aus).

## Ursache gefunden (25.11.2026)
Ein Versuch mit Fehlerauskunft: Minecraft sah die Datei nicht (`sichtbar=None`, Fehler 126). Kein Schutz des Spiels: Dateien, die aus der Claude-App nach `%LOCALAPPDATA%` geschrieben werden, liegen in Wirklichkeit in deren privatem Paketordner (`Packages\Claude_…\LocalCache`). Der Testprozess sah sie nur, weil er daraus gestartet war.
Folge: Testdateien für Minecraft nach `dist\` im Projekt legen, nicht nach `%LOCALAPPDATA%`. Auch die bisher benutzten Einstellungen und Programme liegen dort; außerhalb der Claude-App sind sie unter dem normalen Pfad nicht da.
Warum die Welt „Mett-Debug“ nicht erschien, ist damit nicht geklärt (`%APPDATA%\Minecraft Bedrock` ist nicht umgeleitet).

## Erfolgreich in Minecraft (25.11.2026, 1.26.65.1, Hauptmenü, keine Welt offen)
Vom Projektordner `dist\` aus: Kern und leeres Prüf-Plugin geladen, Stand „läuft“, nach fünf Sekunden restlos entfernt, Spiel reagierte weiter, keine Absturzmeldung. Damit ist der DLL-Grundtest (M0) bestanden. Nicht geprüft: in einer Welt, mit Hooks, auf Servern.

## Hauptthread ohne Hook im Spielcode (25.11.2026, Hauptmenü)
Das Spielfenster (Klasse `Bedrock`) gehört dem Thread, den das Spiel selbst „MINECRAFT MAIN THREAD“ nennt. `plugin-debug` fängt dort über `WH_GETMESSAGE` (eigener Thread, ohne Modul) eine eigene registrierte Nachricht ab: 3 gesendet, 3 im Hauptthread angekommen, danach restlos entfernt, kein Absturz.
Prüfskript mit Schutz: `tools/spieltest.ps1` (genau ein Spiel, keine bzw. nur die genannte Welt benutzt, sonst Abbruch).

## Korrektur Hauptthread
Maßgeblich ist die Fassung mit `WH_CALLWNDPROC` und Halte-Thread (91 Rückrufe in fünf Sekunden, in einer Welt). Die frühere Beobachtung mit `WH_GETMESSAGE` funktionierte nur, solange der Lade-Thread lebte.
