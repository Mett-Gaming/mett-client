# Alles nativ im Spiel (Entscheidung des Nutzers, 25.11.2026)

Vorgabe: „Generell NICHTS mehr via Overlay.“ Anzeige, Menü und Chat laufen künftig als Plugins im Spiel. Das alte Overlay-Fenster ist entfernt.

## Wie die Bildausgabe gefunden wird
Eine eigene Probe-Swapchain verrät die Tabelle von `IDXGISwapChain3` in Windows. Darin werden `Present`, `Present1`, `ResizeBuffers` und `ResizeBuffers1` getauscht. Keine Adressen aus Minecraft, kein veränderter Spielcode. Die Warteschlange des Spiels wird an derselben Stelle gelesen, an der sie bei der Probe liegt, und geprüft (Tabelle, Gerät, Typ). Passt etwas nicht, wird nichts gezeichnet.

## Umbau
| Schritt | Inhalt | Stand |
|---|---|---|
| N1 | Zeichnen im Bild des Spiels | Eigenes D3D12-Fenster und Minecraft-Hauptmenü: läuft. In einer Welt: offen |
| N2 | Eingaben bei offenem Menü oder Chat vom Spiel fernhalten, ohne Fokuswechsel | In Mett-Debug vom Nutzer bestätigt |
| N3 | HUD ins Spiel; FPS aus eigenen Bildern, ohne Windows-Recht | Gebaut, Ersatz-Spiel: geprüft. Minecraft: offen |
| N4 | Menü und Editor ins Spiel | Gebaut. Minecraft: offen |
| N5 | Chat ins Spiel, mit Senden | Gebaut. Minecraft: offen |
| N6 | Alle Einstellungen ohne Neustart | Alles sofort, außer „Minecraft-Chat ausblenden“ (Pack) |
| N7 | Overlay-Fenster entfernen | Entfernt; der Client hat nur noch ein unsichtbares Nachrichtenfenster |

## Geprüft (nur eigenes Testfenster, 25.11.2026)
- Feld erscheint im Bildpuffer, der Rest bleibt unverändert; nach dem Entladen ist alles weg, zweimal nacheinander.
- Größenänderung während des Zeichnens und nach dem Entladen klappt.
- 10-Bit-Format (R10G10B10A2): Direct2D lehnt es ab, das Plugin gibt nach fünf Versuchen auf. Für HDR ist ein Zwischenbild nötig.

## Grenzen
- Jeder harte Fehler im Plugin trifft Minecraft. Discords Hook ist ebenfalls geladen.
- Nur D3D12. Bei anderer Grafikausgabe bleibt das HUD aus.
- Das Bannrisiko auf Servern ist höher als mit dem Overlay.

## Minecraft 1.26.65.1, Hauptmenü, keine Welt (25.11.2026)
- ✅ 15 Sekunden geladen: 2690 Bilder gezeichnet (rund 180 je Sekunde), kein Fehler, Bildformat 28 (R8G8B8A8). Danach Kern und Plugin restlos entfernt, Spiel lebt und reagiert, keine Absturzmeldung.
- Sichtbarkeit des Felds vom Nutzer bestätigt (zweiter Lauf, 2 Minuten). Bildrate ohne Plugin wurde nicht verglichen.

## Eingaben (25.11.2026)
- 🖱️ Maus: Die Roh-Maus (`WM_INPUT`) nutzt Minecraft nur ohne GameInput. Gemessen in Mett-Debug (90 s, nur gezählt): je Bild `GetCurrentReading` (Platz 4) und `GetMouseState` (Platz 14) von GameInput, sonst nichts, rund 550 Aufrufe je Sekunde. Ob dabei Maus und Rad benutzt wurden, ist unbekannt. Tasten laufen dort nicht.
- ⌨️ Tasten: Minecraft liest sie ebenfalls über GameInput (Art 0x10, `GetKeyState` Platz 13, am Spielcode abgelesen). Die Messung zeigte 0, vermutlich weil niemand eine Taste drückte.
- 🔒 `plugin-hud/mouse.rs`: steht vor `GetMouseState` und `GetKeyState`. Bei offener Eingabe steht die Maus und keine Taste ist gedrückt; danach wird die Bewegung abgezogen, je Maus getrennt. Esc und Eingabe bleiben nach dem Schließen mindestens 250 ms gesperrt, die Öffnen-Taste immer. Passt der Aufbau (56 Byte) nicht, bleibt die Maus unverändert.
- 🐞 Zweiter Lauf laut Nutzer: Kamera sprang nach unten (alle Mäuse teilten einen Ausgleich), Esc öffnete das Pausenmenü und die Zeile blieb offen. Behoben und vom Nutzer bestätigt (dritter Lauf): Ausgleich je Gerät, Tasten gesperrt, sichtbar und gesperrt sind jetzt ein Zustand, Esc schließt zur Not auch über GameInput.
- 🖱️ Wie in Java ist der Mauszeiger bei offener Eingabe frei und wird beim Schließen zurückgegeben.
- ✅ Erster Lauf in Mett-Debug laut Nutzer: F8 und Tippen in Ordnung, Kamera blieb frei (damals noch ohne Maussperre).
- 🧱 `plugin-hud/input.rs` setzt sich vor die Fensterfunktion und reicht bei offener Eingabe Maus, Klicks, Mausrad und Tasten nicht weiter. Vorher Gedrücktes darf losgelassen werden. Alt-Kürzel bleiben beim Spiel. Fällt die Anzeige aus, wird die Eingabe sofort frei.
- 🧪 Probe: F8 öffnet eine Eingabezeile, die nichts sendet.
- ✅ Der einmalige Testausfall ist erklärbar: Beim zweiten Auftreten war Strg gedrückt (Sprinten), und der Test verglich Umschalttasten. Dabei fiel ein echter Fehler auf: Mit gehaltener Strg-Taste öffnete sich der Chat nicht. Behoben.

## Aufteilung (25.11.2026)
Das Plugin ist Bildschirm und Eingabe, der Client bleibt der Kopf. So laufen keine Datenquellen im Spiel.
- 🔗 `hudlink`: gemeinsamer Speicher `MettHud.<PID>`. Hin: Zeichenbefehle (`hud::wire`, alles geprüft) und Wünsche (Chat-Tasten, Menü offen). Zurück: Eingaben, Bildgröße, Bildzähler. Nichts davon auf der Platte.
- 💓 Der Client meldet sich jeden Takt. Schweigt er zwei Sekunden, zeichnet das Plugin nichts mehr und gibt Maus und Tasten frei.
- 🔌 Der Kern lädt fehlende Plugins nach; der Client nimmt beim Beenden alle zurück.
- 🧹 Entfernt: Overlay-Fenster, Fokus-Tricks, Tastatur-Hook, Eckknopf, FPS über ETW (Quelle noch vorhanden, unbenutzt).
- 🧪 `examples/durchlauf` lädt die echten DLLs in ein Ersatz-Spiel (`examples/ersatzspiel`): laden, nachladen, zeichnen, tippen, Client weg, neuer Client, entladen. Alle 17 Prüfungen bestanden.
- 🚧 Grenze: Wird „Eigene Eingabe“ abgeschaltet, bleibt das Chat-Plugin ungenutzt geladen, bis der Client endet. Stirbt der Client hart, bleiben Kern und Plugins bis zum Spielende untätig geladen.

## Erster Lauf in Minecraft (25.11.2026, 1.26.65.1)
- ✅ Laut Nutzer klappt alles Geprüfte, auch Befehle (zum ersten Mal ausgeführt). Einzelne Punkte der Prüfliste wurden nicht getrennt bestätigt.
- 🐞 Gesendete Nachrichten waren anfangs nicht zu sehen: Die Liste ist erst erkennbar, wenn etwas darin steht, und die Suche pausierte bis zu 60 s. Jetzt: nach dem Senden sofort, sonst höchstens 15 s; was nach einer leeren Suche da ist, gilt als neu. Im Spiel noch zu bestätigen. Die erste fremde Nachricht kann weiter bis etwa 17 s brauchen.

## Minecrafts Chat-Anzeige verschieben (25.11.2026)
- 🧭 Versuch in „Mett-Debug“ vom Nutzer gesehen: `chat_panel` bewegt sich, beim Entladen steht es wieder am alten Platz. Ohne Merker „neu ausrechnen“ passiert nichts, die Werte allein reichen nicht.
- 🔎 `gamemem::uitree`: Einstieg über die Namen `chat_panel`, `chat_stack`, `root_panel`. Abstände (Lage, Größe, Eltern, Kinder, Merker) werden je Spielstart aus Tausenden Elementen abgeleitet; nicht eindeutig heißt aus. Am Spiel nur lesend geprüft: zehnmal dasselbe Ergebnis, rund 0,9 s, danach 0,35 s je Welt.
- 🧩 Plugin `chatui`: schreibt nur im Hauptthread, prüft vor jedem Schreiben Klasse, Namen und Eltern. Endet der Client, stellt es von selbst zurück.
- 🔀 Einstellung „Minecrafts Anzeige hierher schieben und benutzen“ (anfangs aus): unterer Rand folgt dem Chat-Modul. Bei offenem Chat zeigt unser Chat den Verlauf, Minecrafts Anzeige ist so lange außerhalb des Bildes.
- ⏳ Offen: Java-Aussehen (Hintergrund, Breite, Schrift, Durchsichtigkeit), Symbole im Verlauf, andere Auflösungen, Server.
- 🔢 Höchstzahl an Nachrichten (1 bis 40, anfangs 10): Minecraft begrenzt die Liste `stack_panel` nicht sichtbar, deshalb rückt das Plugin ältere `chat_grid_item` aus dem Bild. Zurückgestellt wird wie bei der Anzeige selbst.
- 🖼️ Im Editor stehen Beispiele in Minecrafts Maßen, der Rahmen hat dieselbe Größe; die echte Anzeige ist so lange aus dem Bild.
- 🎞️ „Nachrichten sanft ausblenden“ (anfangs an): aus heißt, eine Zeile rückt aus dem Bild, sobald die Deckkraft ihres Texts sinkt. Erscheinen hat keine Animation (gemessen).
- 〰️ Bekannt: Kommt oder geht eine Nachricht, springt der Chat für ein Bild. Das Spiel legt die Liste neu an und zeichnet sie, bevor das Plugin wieder dran ist. Versucht und verworfen: Mindesthöhe der Anzeige festhalten, das Spiel setzt sie bei jeder Neuberechnung zurück.
- 🔬 Versuch (Plugin `uiprobe`, nur Entwicklung): leere Rückrufe an den Bausteinen von Anzeige und Liste gezählt. Platz 5 kommt beim Entfernen einer Nachricht im Hauptthread, am letzten Baustein der Liste schon mit der neuen Höhe. Beim Hinzufügen kommt nur Platz 4, mit der alten Höhe.
- 🪝 Gegen das Wackeln: `chatui` legt den Bausteinen der Nachrichtenliste eine Kopie ihrer Funktionstabelle unter, in der nachweislich leere Funktionen (`ret`, `xor al,al; ret`) auf uns zeigen. Kein fester Platz: Jeder Rückruf prüft nur, ob sich die Höhe geändert hat, und rückt dann nach. Die Bausteinliste wird je Spielstart abgeleitet; fehlt sie, bleibt nur diese Glättung aus. Beim Erscheinen einer Nachricht kommt kein Rückruf mit neuer Höhe.
- ✅ Vom Nutzer in „Mett-Debug“ abgenommen: kein Wackeln mehr beim Verschwinden. Entladen mit aktiven Rückrufen geprüft.
- 📏 Breite frei: Im Layout-Baustein steht je Maß ein Objekt mit dem ausgerechneten Wert (`gamemem::uisizes`, je Spielstart aus der fertigen Größe abgeleitet). Das Plugin setzt ihn bei `chat_panel` und `stack_panel`; neue Zeilen richten sich danach. Verworfen: die Größenregel selbst, sie wird nur beim Anlegen übersetzt. Die Höhe begrenzt, welche Nachrichten zu sehen sind.
- 🧹 Auf Wunsch des Nutzers: keine eigene Nachrichtenanzeige mehr, auch kein Verlauf bei offenem Chat. Eigen sind Eingabe, Befehlshilfe und Meldungen wie „Nicht gesendet“. Nachrichten werden nicht mehr gelesen. „Minecraft-Chat ausblenden“ ist entfallen.
- 📜 Auf späteren Wunsch des Nutzers wieder da: Verlauf bei offener eigener Eingabe (abschaltbar). Gelesen wird nur, solange sie offen ist, danach ist alles verworfen; nichts wird gespeichert. Die Liste wird bei jedem Öffnen neu gesucht (rund 0,2 s, eindeutig oder aus). Minecrafts Anzeige ist so lange außerhalb des Bildes. Mausrad 7 Zeilen, mit Umschalt 1, Bild auf/ab.
- 🔤 Schriftgröße: Minecraft bietet sie nur mit Noto Sans an (Angabe des Nutzers). Entscheidung: kein Noto Sans; `chat_typeface` steht beim Start über den Launcher auf 0 und wird danach zurückgestellt.
- ☕ „Aussehen wie in der Java Edition“ (anfangs aus): Zeilenhöhe 9 und Hintergrund 50 % über die benannten Einstellungen `chat_line_spacing` und `gfx_chat_background_opacity` im laufenden Spiel (`gamemem::options`, Wertfeld durch Abgleich mit `options.txt`), Text 4 eingerückt. Plugin stellt beim Entladen zurück; ein Merkzettel (`mett_chat_style_restore`) stellt auch die Datei zurück, falls Minecraft unsere Werte speichert.
- 👥 Tab-Menü (anfangs aus): Namen aus Minecrafts Spielerliste (`gamemem::players`), erkannt an ihrer Form: geschlossener Ring, jeder Eintrag mit doppelter Kennung, Name und Xbox-Kennung; im Einzelspieler müssen beide Listen gleich sein. Verworfen: die eigene `last_xuid` als Einstieg, auf Servern steht der eigene Eintrag nicht immer in der Liste. Namen werden nur bei gehaltener Taste gelesen. Darstellung nach Java 26.3 (Maße am Code geprüft) mit Gesicht aus dem Skin (8 × 8, hängt am Listeneintrag); Ping unbekannt, sein Platz bleibt frei. Farben stehen auf Lifeboat nicht in der Liste, Namen sind dann weiß (Entscheidung des Nutzers). Tab erreicht Minecraft beim Spielen nicht.
- 🔭 Zoom (Modul, anfangs aus; Minecraft begrenzt das Sichtfeld beim Zeichnen mit der Untergrenze aus dem Einstellungs-Objekt, das Plugin senkt sie solange mit): Verhalten und Einstellungen nach Zoomify (Werte und Formeln nachgelesen, LGPL, eigene Umsetzung in `hud::zoom`): 20 Takte je Sekunde mit Zwischenbildern, Verläufe, Mausrad-Stufen, Umschalten, Stufe behalten, relative Maus, Film-Kamera, Zweit-Zoom (ohne Taste). Plugin `zoom` setzt das benannte `gfx_field_of_view`, `hud` liefert Tasten und bremst die Maus. Nicht möglich: Hand, Gehbewegung, Fernrohr. Merkzettel `mett_zoom_restore` gegen ein mitten im Zoom gespeichertes Sichtfeld; der Wert darin kommt aus dem laufenden Spiel, solange nicht gezoomt ist (bei mehreren Konten nur aus der Datei).
- 🧩 Spielerliste ist jetzt auch ein Modul; der alte Schalter wird beim Laden übernommen.
- 🧪 Frischer Start über `--launcher` und Aufräumen nach dem Beenden vom Nutzer bestätigt und geprüft (25.11.2026): Einstellungen, Pack und Merkzettel restlos zurück. Behoben: Reste werden jetzt schon vor dem Start zurückgestellt.
- 🍎 **AppleSkin:** Sättigung und Erschöpfung an der Hungerleiste. Einstieg sind die Namen der Werte (`minecraft:player.hunger` …) und `hunger_rend` für die Lage. Bei anderen Spielern stehen Hunger, Sättigung und Erschöpfung auf der Vorgabe; nur der eine abweichende Block wird als eigener gelesen, sonst bleibt es aus. Im Client, liest nur.
- 🪟 Ohne Fokus bleibt das HUD stehen: Gezeichnet wird im Spiel, also entscheidet nicht mehr, welches Fenster vorne ist. Tasten und Klicks zählen nur, solange Minecraft vorne ist; Editor und Chat schließen weiterhin beim Wechsel.
- 🖼️ AppleSkin zeichnet Ausschnitte der originalen `icons.png` (`assets/appleskin`, Unlicense). Der Erschöpfungsbalken liegt im Original hinter den Keulen; unserer liegt darüber und spart sie aus.
- ⚡ Leistung: Speichersuchen laufen mit niedrigster Priorität. AppleSkin sucht die Blöcke selten (höchstens jede Minute) und vergleicht sonst nur die gefundenen; die Spielerliste sucht ebenso höchstens jede Minute. Das HUD misst seine Zeit je Bild (`METT_HUD[4]`, `[5]`, Beispiel `zeichenzeit`).
- ➕ Eigenes Crosshair blendet Minecrafts Fadenkreuz aus: Element `cursor` unmittelbar in `hud_crosshair_screen`, gefunden über die Namen, vom Plugin `chatui` (eigener Auftrag `mett_cursor_order`) nur im Hauptthread über die Merker „selbst sichtbar“ und „samt Eltern sichtbar“ ausgeblendet und beim Ausschalten oder Ende wieder eingeblendet. Wegrücken wirkt nicht: Es wird immer in der Bildmitte gezeichnet. Die Merker werden je Spielstart aus der Regel „samt Eltern = selbst und Elternteil“ gelernt. Nicht eindeutig heißt: Es bleibt sichtbar.
- 📶 Ping ohne Servereintrag: Die Adresse kommt aus der laufenden Verbindung (`gamemem::server`, Einstieg Klassenname `webrtc::ProxyConnection`, genau eine öffentliche Adresse), nur im Arbeitsspeicher. Gemessen wird mit ICMP, sonst mit der Serverabfrage von Minecraft. Geht bei Servern, die Minecraft über WebRTC verbindet (geprüft: Lifeboat); ältere RakNet-Verbindungen und IPv6 werden nicht erkannt, dann steht „Kein Server“.
- 💡 Fullbright (Modul, anfangs aus): stellt `gfx_gamma` über eine feste Liste erlaubter Namen um (`mett_plugin::values`, Auftrag `mett_value_order` im Plugin `chatui`, Hauptthread, vor jedem Schreiben geprüft). Liegt der Wunsch außerhalb der Grenzen der Einstellung, werden sie solange erweitert. Merkzettel `mett_<name>_restore`.
- 📍 Beim Start über den Launcher werden vier Größen von Touch-Knöpfen (`focus::MARKS`) vor dem Spielstart um 0,01 verstellt und nach dem Spiel zurückgestellt (Zettel je Einstellung; eigene Wahl des Nutzers bleibt). Grund: Stehen fast alle Zahlen auf Standard, sind Wert und Standard sonst nicht zu unterscheiden (bei einem Nutzer belegt: 93 von 93 gegen 92).
- ⚡ Benannte Einstellungen werden je Spielstart einmal gesucht (`named.rs`) und von Java-Stil, Zoom und Fullbright gemeinsam benutzt. Plugins werden beim Start über den Launcher immer geladen; Module schalten nur noch um. Verworfen: `gfx_hotbarScale` wirkt nur bei Touch, `options.dev_renderBoundingBox` (Hitboxen) lässt sich umlegen, zeichnet in der Verkaufsfassung aber nichts.
- 🛡️ Rüstung, Effekte und Gegenstandszähler (Module, anfangs aus, nur lesen): `gamemem::inventory`. Einstieg sind die Gegenstandsnamen mit ihrer Prüfsumme (FNV); Abstände zur Art und zwischen den Plätzen werden je Spielstart gelernt. Nur Reihen, deren Stapel alle eine Nummer des Servers tragen, gelten als eigene (36 Plätze Inventar, 5 Plätze Rüstung). Effekte: Reihe mit einem Platz je Nummer, die jüngste Kopie gilt. Nicht eindeutig heißt aus. Stellen innerhalb eines Stapels sind fest und werden an jeder Reihe geprüft. Symbole von game-icons, nichts aus dem Spiel.
- 🖼️ Bilder von Gegenständen, Effekten und Schnellleiste kommen zur Laufzeit aus der Installation des Nutzers (`.brarchive`, offene Behälter mit PNG und TGA), wie die Schrift; nichts davon wird mitgeliefert oder gespeichert. Armor HUD nach „uku's Armor HUD“ (MIT), Lage über das Element `desktop_hotbar`.
