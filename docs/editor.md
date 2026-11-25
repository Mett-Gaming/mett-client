# HUD-Editor (M2)

Bedienung nach Vorbild des NoRisk Client, Aussehen eigenes Mett-Design. Kein Code und keine Grafiken von dort.

## Belegt (NoRisk-Wiki, Übersetzungen, Meldungen)
- Rechte Umschalttaste: kompakte Ansicht über dem Spiel. Doppelt drücken oder linke + rechte Umschalttaste: volle Ansicht. Taste belegbar.
- Module werden direkt auf dem Bildschirm gezogen; ein Stift am Modul führt zu seinen Einstellungen.
- Einstellungen je Modul: Größe, Abstand, Hintergrund, Schrift-, Umriss- und Druckfarben, Umriss, Inhalte ein/aus.
- Änderungen lassen sich global auf alle Module anwenden.

## Besser machen
- Lage über Anker statt fester Pixel: Fensterwechsel verschiebt nichts (dort gemeldet: 120).
- Beim Schließen des Editors bewegt sich nichts (dort gemeldet: 1463).
- Vorschau und Spiel benutzen dieselbe Zeichnung, kein Ein-Pixel-Versatz (dort gemeldet: 2625).

## Noch offen
Der Quelltext des eigentlichen Clients war öffentlich nicht zu finden, nur Launcher und Nebenprojekte (GPL-3.0).
Nicht belegt: Einrasten, Hilfslinien, Skalieren per Ziehen oder Mausrad, Mehrfachauswahl, Rechtsklick.

## Umgesetzt (M2)
- Eigene Standards, wo NoRisk nicht belegt ist: Einrasten mit Hilfslinien, Mausrad skaliert um die Mitte, Rechtsklick oder Stift öffnet die Einstellungen, Pfeiltasten, Strg+Z.
- Profile (höchstens 7), Teilen über Ordner, vier Themes, Crosshair, Knopf in Minecraft-Menüs in wählbarer Ecke.
- Der Knopf erkennt Menüs am Mauszeiger und erscheint deshalb auch im Inventar und Chat.

## Offen
- Automatischer Profilwechsel je Server: von außen ist der Server nicht erkennbar, kommt frühestens mit M4.
- Rechte Umschalttaste und echter Klick auf den Knopf: von Hand zu prüfen.

## Nativer Knopf im Pausenmenü
`pack/` ergänzt die Knopfliste gezielt; die Vanilla-Datei wird nicht ersetzt. Bricht ein Update die Stelle, fehlt nur der Knopf.
Im Spiel bestätigt (1.26.65.1): Der Knopf erscheint an der richtigen Stelle.
Verworfen: Link `mettclient://` über `menu_open_uri`. Windows öffnet ihn, Bedrock löst ihn im Pausenmenü nicht aus.
Die Textur unter dem Zeiger braucht eine Nineslice-Datei wie bei Vanilla. Ohne sie behält Bedrock das Seitenverhältnis
bei und füllt nur ein Quadrat in der Knopfmitte (im Spiel gesehen). `marker.rs` prüft die Farbe beim Klick.
Im Spiel bestätigt (1.26.65.1): 100 % Farbe unter dem Zeiger, Editor geht auf. Der Eckknopf ist deshalb standardmäßig aus.
Der Knopf löst zugleich `button.menu_continue` aus: Pausenmenü zu, danach Editor auf. Im Spiel bestätigt, Editor blieb offen (7 s).

### Nur über den Mett Launcher
- `MettClient.exe --launcher` (Katalogfeld `args`) legt vor dem Spielstart Pack und Listeneintrag an.
- Ein losgelöster Aufräumer entfernt alles, sobald Minecraft und Mett-Client beendet sind, auch nach einem Absturz.
- Nach Stromausfall oder erzwungenem Neustart bleibt es bis zum nächsten Start von Mett-Client liegen.
- Lief Minecraft schon, wird nichts geändert: Packs liest es nur beim Start.
- Fremde Packs und unlesbare Listen bleiben unberührt. Ein Fehler kostet nur den Knopf.

## Menü im NRC-Aufbau
Seitenleiste, Reiter, Suche, zweispaltige Karten (Klick schaltet, … oder Rechtsklick öffnet), Detailseite mit Zurück, An/Aus und Reset. Farben, Schrift, Symbole und Texte sind von Mett.
Nicht übernommen: Unschärfe des Hintergrunds (bräuchte eine Bildschirmaufnahme), Freunde, Cosmetics und Ähnliches. Wegpunkte folgen mit M4.

## M3
Module: FPS, System, Akku, Musik, Ping, Spielzeit, Stoppuhr (F9/F10), Notiz; alle anfangs aus. Jede Quelle läuft für sich und misst nur, wenn sie angezeigt wird.
Am Testfenster geprüft: System, Akku (keiner), Musik, Ping zum eigenen Rechner, Spielzeit, Notiz, Meldung ohne FPS-Recht.
Offen: FPS-Werte, Discord-Anzeige über den Launcher, alles in Minecraft.
- ↔️ Module haben Breite und Höhe unabhängig vom Maßstab (`Placement.w/h`), gezogen an 4 Seiten und 4 Ecken auf ganze Pixel; Mausrad ändert keine Größe mehr. Der eigene Chat bricht nach Breite um und zeigt so viele Zeilen, wie passen. Bei Minecrafts Anzeige ändert die Höhe die Zahl der Nachrichten, die Breite ist fest.
- 📐 Textfelder haben alle dieselbe Vorgabegröße (130 × 30; System, Musik und Notiz 230 breit). Einstellungen unter Version 4 werden einmal darauf gesetzt, Plätze bleiben; Tasten und Chat sind ausgenommen.
- 🧹 „Alles zurücksetzen“ (Optionen, zwei Klicks): alle Einstellungen auf Anfang, ein Profil, alle Module und das Crosshair aus. Vorher Sicherung `vor-dem-zuruecksetzen.bak` im Profil-Ordner; ohne Sicherung passiert nichts.
