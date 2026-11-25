# Machbarkeit (M0), Bedrock 1.26.65.1, 2560×1440

| Prüfung | Ergebnis |
|---|---|
| Start über App-Kennung | ok |
| Overlay über Minecraft, fensterfüllend 2560×1440 | ok, Hauptmenü |
| Klicks gehen durch | ok, 3 Punkte per Fenstertreffer geprüft |
| Lesen mit normalem Leserecht | ok, nur Programmkennung gelesen |
| Keine `mett*`-Module in Minecraft | ok |
| Overlay schließen lässt Minecraft offen | ok, Code 0 |
| Ende des Zielprogramms beendet Mett-Client | ok, mit ping.exe geprüft |
| HUD in Menüs aus | ok im Hauptmenü |
| Aufwand im Hauptmenü mit sichtbarem HUD | rund 1,6 % eines Kerns, 63 MB |

HUD ingame sichtbar und im Menü weg: ok. Absturz 0x87A beim Schließen nach sichtbarem HUD: behoben, 3 Wiederholungen mit Code 0 (erklärt auch die zwei Prozesse im ersten Test).

Offen: CPS-Werte, Fenstermodus, DPI-Wechsel, FPS-Vergleich, DLL-Test in Offline-Welt, Controller.
DLL-Grundtest bestanden, siehe docs/loader.md (25.11.2026, Hauptmenü).
