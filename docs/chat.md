# Mett-Chat: Stand der öffentlichen Vorarbeit (25.11.2026)

Wunsch: Vanilla-Chat aus, eigenes Overlay mit gleicher Funktion. Entschieden: über Code im Spiel.

| Projekt | Lizenz | Zuletzt | Für uns |
|---|---|---|---|
| LeviLamina (Client) | LGPL-3.0 | 22.11.2026, v26.60.9 | Mod-Lader mit Headern und Adressdaten; Client-Daten laut Bau-Datei für 26.60.5 |
| LeviLauncher | GPL-3.0 | 20.11.2026 | Eigene Spielinstanzen, Start per Befehl (`play`) |
| Lamium | LGPL-3.0 | 24.11.2026 | Beispiel für einen Client-Mod; nennt 1.26.60.05 |
| Latite | GPL-3.0 | 03.11.2026 | Offener Client mit Chat-Ereignissen; Version nicht geprüft |
| Flarial (dll-oss) | AGPL-3.0 | 21.03.2026 | Laut README verzögerte Adressen |
| Amethyst | Apache-2.0 | 16.10.2026 | Ab 1.26 Typen selbst mitbringen |

## Für den Chat vorhanden (LeviLamina-Header, nicht ausgeführt)
- Empfangen: `GuiData::displayChatMessage`, `displayClientMessage`, `_addMessage`, `displaySystemMessage`, `displayWhisperMessage`
- Verlauf: `ClientInstanceScreenModel::getPreexistingChatMessages`
- Senden: `ClientInstanceScreenModel::sendChatMessage`

## Grenzen
- Installiert ist 1.26.65.1, gefunden wurden Daten für 26.60. Ob das passt, ist ungeprüft.
- `isChatMute`/`setMuteChat` stehen im Header. Ob sie nur die Anzeige betreffen, ist ungeprüft.
- Laut Bedrock-Native-Modding-Wiki ist die Programmdatei auf der Platte verschlüsselt. Meine frühere Suche nach Klassennamen darin sagt deshalb nichts aus.
- Das Wiki nennt einen Weg, sie entschlüsselt zu kopieren. Das nutzen wir nicht (AGENTS.md: kein Umgehen von Schutzmechanismen).
- Wie LeviLauncher das Spiel bereitstellt, ist vor einer Nutzung zu prüfen.
- Kein Code aus GPL-/AGPL-Projekten übernehmen: Das würde die Lizenz von Mett-Client mitbestimmen.

## Quellen
github.com/LiteLDev/LeviLamina, github.com/LiteLDev/LeviLauncher, github.com/amatouhake/Lamium, github.com/LatiteClient/Latite,
github.com/flarialmc/dll-oss, github.com/FrederoxDev/Amethyst, bedrock-native-modding.github.io/wiki

## Nachprüfung LeviLamina (25.11.2026)
- LeviLauncher nutzt nicht die Store-Installation, sondern lädt das Spielpaket selbst und entschlüsselt es (`internal/nativeinstall/extract.go`, AES-Inhaltsschlüssel aus der eigenen Store-Lizenz). Ein Konto mit gekauftem Spiel bleibt nötig.
- Der Lader wird in die Importtabelle der Spieldatei eingetragen, sie wird also verändert.
- Das widerspricht unserer Regel „kein Umgehen von Schutzmechanismen“. Ohne Änderung der Regel durch den Nutzer wird dieser Weg nicht benutzt.
- LeviLamina 26.60.9 verlangt `bedrock-runtime-data 26.60.5-client.17`; Client-Daten für 26.65 wurden nicht gefunden.

## Neuer Weg (Entscheidung 25.11.2026: eigene DLL, LeviLamina nur als Referenz)
- Empfangen zuerst rein lesend versuchen: Nachrichten als Struktur im Speicher (`gamemem::chat`, rechnerisch 272 Byte). Ohne Injection.
- DLL nur für das Senden. Keine Entschlüsselung von Spieldateien: im laufenden Spiel nicht nötig.
- Suchwerkzeug: `cargo run --release -p gamemem --example chatsuche -- <Testwort>`; gibt nur das eigene Testwort aus.

## Empfangen: bestätigt an 1.26.65.1 (25.11.2026)
- Aufbau stimmt (272 Byte); die Anzeigedauer zählt nach dem Ausblenden ins Negative.
- Liste ohne Testwort eindeutig gefunden, 1,8 s für etwa 2,4 GB; drei neue Nachrichten kamen live an. Nur gelesen, Einzelspieler.
- Modul „Chat“ im HUD, anfangs aus. Offen: Server, Farben, Ausblenden des Vanilla-Chats, Senden, Scrollen.
- Anzeige im HUD vom Nutzer bestätigt.
- Option „Minecraft-Chat ausblenden“: Pack setzt `hud.chat_panel` auf `ignored`, nur beim Start über Mett und nur mit eingeschaltetem Chat-Modul. Im Spiel ungeprüft, auch ob das Mitlesen dann weiterläuft.

## Ziel (Vorgabe des Nutzers, 25.11.2026)
1. Originaler Chat komplett aus. 2. Overlay im Stil und an der Stelle des Java-Chats, verschiebbar. 3. Tasten für Chat und Befehl öffnen unseren Chat, inklusive Senden.
Verworfen vom Nutzer: Minecrafts Chatfenster unsichtbar im Hintergrund weiterbenutzen (zu träge). Eingabe und Anzeige sind komplett bei uns.

## Spuren für das Senden (1.26.65.1, laufendes Spiel, nur gelesen)
- Abbild lesbar (238 MB Code, 52 MB Daten); 15.523 Klassennamen (`.?AV`) vorhanden. Frühere Suche in der Datei war wegen Verschlüsselung ohne Aussage.
- Namen von Lambdas aus `ChatScreenController` (`_registerBindings`, `_pasteClipboardInChat`, Konstruktor) sind da; ihre Einträge führen zu Code. `ClientInstanceScreenModel` ebenfalls.
- `sendChatMessage` selbst kommt nur in einem Vorlesetext vor, ist also kein direkter Anker.
- Weg: Lambda-Name → Typeintrag → Sprungtabelle → Funktion → Aufruf des Sendens. Noch nicht verfolgt.
- Tasten laut options.txt: Chat T und Enter, Befehl Taste 191.
- Schrift: `data/fonts/Mojangles.ttf` der Installation zur Laufzeit laden, nicht mitliefern.

## Sendefunktionen gefunden (1.26.65.1, 25.11.2026, nur gelesen, nicht aufgerufen)
Kette ohne feste Adressen (`tools/sendesuche/suche.py`), jede Stufe genau ein Treffer:
1. FNV-1a-32 von `button.send` als Konstante im Code, 2. Funktionstabelle davor, 3. Knopf-Handler, 4. Senden des Chat-Controllers,
5. Prüfung auf `/`, 6. zwei Aufrufe danach: Befehl und Chat.
- Chat: (Modell, Text) wie `ClientInstanceScreenModel::sendChatMessage`; Rückgabe 1 heißt eingeschränkt. Einziger Aufrufer ist der Chat-Controller.
- Befehl: (Text, Zeiger auf Spielinstanz).
- Das Modell braucht Zeiger bei +0x48/+0x58 (aus dem Code gelesen).
Offen: passendes Modell zur Laufzeit, Aufruf im richtigen Thread des Spiels, DLL-Test. Kennungen gelten auch für `button.chat_*`.

## Senden bestätigt (25.11.2026, 1.26.65.1, nur Welt „Mett-Debug“, Mehrspieler aus)
Das Debug-Plugin rief `sendChatMessage` im Hauptthread auf: Rückgabe 0, Chatliste vorher 1, nachher 2 Nachrichten, Text `METTPLUGIN1` darin. Kein Absturz, Plugin und Kern restlos entfernt.
- Modell über Namen: Lambdas aus `?0HudScreenController@@…ClientInstanceScreenModel` → Konstruktor → Tabelle → lebender Controller → Modell. Der Feldversatz (0xd48) wird aus dem Sende-Code gelesen. Ergebnis war eindeutig (`tools/sendesuche/ziel.py`).
- Hauptthread: `WH_CALLWNDPROC` mit eigenem Halte-Thread. Ohne ihn entfernt Windows den Hook mit dem Ende des einrichtenden Threads; das war die Ursache der fehlgeschlagenen Anläufe.
- Davor mehrere Anläufe ohne Wirkung (nichts gesendet, Auftrag blieb liegen).
Offen: Befehle (`/`), Umlaute und lange Texte, Server, Verhalten beim Weltwechsel, Suche im Plugin statt im Analyseskript, eigene Eingabe und Tasten.

## Tasten und Modellsuche (25.11.2026)
- ⌨️ `examples/tastentest.rs` fängt T, Enter und die Befehlstaste beim Spielen ab: drei Anschläge verschluckt, der Minecraft-Chat blieb laut Nutzer zu. Weder Plugin noch geänderte Tastenbelegung nötig.
- 🔎 Modellsuche in Rust (`compat::hud_model`, `gamemem::model`), am laufenden Spiel nur lesend geprüft: dasselbe Ergebnis wie das Analyseskript, genau ein Modell, etwa 4 s. Das Aufrufen mit diesem Ergebnis ist noch nicht verdrahtet.
- 📐 Maße und Ausblenden in `hud::java_chat` nach Java 26.3 nachgebaut, eigene Umsetzung. Im Spiel noch nicht abgenommen.

## Eigene Eingabe und Sende-Plugin (25.11.2026)
- ⌨️ `chat_keys`: fängt die Chat-Tasten aus `options.txt` nur ab, wenn Senden bereit ist, sonst öffnet sich Minecrafts Chat. Bei offenem Chat hat das Overlay den Fokus; Alt+Tab, Windows- und F-Tasten bleiben beim System.
- ✏️ `hud::chat_input`: Eingabe im Java-Stil, 256 Zeichen, Auswahl, Zwischenablage, Verlauf (nur im Arbeitsspeicher), 20 Zeilen mit Rollbalken.
- 📨 `plugin-chat`: ein Auftrag je abgeschickter Nachricht, im Hauptthread neu geprüft (Controller, Tabelle, Modell, Funktionsanfang). Kommt er nicht rechtzeitig an, wird er zurückgezogen und nie nachgeholt. Test mit nachgestelltem Spielfenster: kurze und lange Texte, Umlaute, sechs Abweichungen, Entladen.
- 🔗 `chat_link`: sucht und lädt nur mit `--launcher`, eingeschaltetem Chat-Modul und der Option „Eigene Chat-Eingabe“ (anfangs aus). Nach einem Weltwechsel wird neu gesucht. Stand und Grund stehen auf der Chat-Seite im Menü.
- 🚧 Offen: Server, Weltwechsel, Vorschläge. Das Lebendzeichen des Modells stützt sich auf Feldprüfungen; freigegebener, unveränderter Speicher wäre damit nicht sicher zu erkennen.
- ✅ Vom Nutzer in „Mett-Debug“ bestätigt (1.26.65.1): T öffnet unsere Eingabe, Senden klappt. Einzeln nicht bestätigt: Maus bei offenem Chat, hängende Tasten, Weltwechsel.

## Befehle und Kamera (25.11.2026, im Spiel noch nicht geprüft)
- ⌨️ Befehle: `(Text mit /, Zeiger auf {Lebendzeichen, Zähler, Instanz})` aus Modell +0x48/+0x50/+0x58. Der Aufrufer erhöht den Zähler bei +8, die Funktion senkt ihn am Ende selbst. `compat` prüft diese Form im Sende-Code; sieht sie anders aus, gehen nur Nachrichten. Im laufenden Spiel gefunden, nicht aufgerufen.
- 🧪 Nachgestelltes Spiel: Befehl kommt an, Zähler danach unverändert, unsinniger Zähler wird abgelehnt.
- 🎥 Die Maus abzufangen hielt die Kamera laut Nutzer nicht an: `GameInputRedist.dll` ist im Spiel geladen, die Maus läuft vermutlich darüber und geht nur an das Programm im Vordergrund. Deshalb nimmt das Overlay bei offenem Chat den Fokus und gibt ihn danach zurück. Bestätigt: Minecraft öffnet dabei sein Pausenmenü, wenn `pause_menu_on_focus_lost:1` in `options.txt` steht. Der Client nimmt den Fokus deshalb nur bei `0`, sonst bleibt die Kamera beweglich.
- ⏸️ Beim Start über den Launcher wird `pause_menu_on_focus_lost` vor dem Spielstart auf 0 gestellt und nach dem Spiel zurück auf 1 (Wunsch des Nutzers). Ein Merkzettel neben `options.txt` sichert das auch nach einem Absturz; stand sie schon auf 0, bleibt sie so. Läuft Minecraft schon, wird nichts geändert.

## Befehlshinweise (25.11.2026)
- 📜 `gamemem::commands` liest die Befehlsliste der Client-Seite aus dem Speicher, also genau das, was der Chat des Spiels vorschlägt, samt eigenen Befehlen von Servern und passend zu den Rechten. Einstieg über `help`/`closechat`; Client-Seite heißt: kaum Ausführfunktionen. Werte und Wertelisten müssen zueinander und zu den Parametern passen, genau auf eine Weise, sonst keine Hinweise.
- 🔎 Am laufenden Spiel (Mett-Debug): 80 Befehle, 15.046 Werte, gefunden in etwa 0,6 s, gelesen in 12 ms. Auf Servern ungeprüft.
- 🧩 Freie Parameter kennt das Spiel nur als Art. Was eine Art bedeutet (Ziel, Koordinaten, Text, Zahl, Befehl), wird an bekannten Befehlen gelernt und gilt dann auch für Serverbefehle. Widersprüche zählen nicht. Fehlen die bekannten Befehle, gilt ein einzelnes Wort.
- 🎨 `hud::hints`: Liste, Pfeiltasten, Tab, Maus, blasser Rest, Parameterzeile und Farben nach Java 26.3, eigene Umsetzung. Spielernamen werden nicht vorgeschlagen.

## Chat ab der ersten Sekunde (Pflicht des Nutzers, 25.11.2026)
- ⚡ Auswerten und Plugin laden passieren schon im Hauptmenü; T gehört uns, sobald Senden mit dieser Spielversion geht. Die Stelle in der Welt wird mit mehreren Threads gesucht (Auswertung und Suche zusammen 1,3 s statt 4,1 s).
- 📨 Abgeschicktes wartet bis zu 10 s auf die Suche, der Reihe nach; danach eine rote Meldung und nie ein späteres Senden.
- 🖱️ Nach dem Schließen blieb der Zeiger in Menüs unsichtbar: Das Zeigerbild wurde gelöscht statt zurückgestellt. Behoben.
- ✅ Vom Nutzer als Ganzes bestätigt (1.26.65.1): Hinweise, sofortiger Chat, Mauszeiger. Serverbefehle und frischer Spielstart wurden nicht einzeln genannt.

## Serversymbole und kleine Server (25.11.2026, Lifeboat)
- 🔣 Zeichen wie `U+E4DF` sind Bilder aus dem Pack des Servers (`glyph_E4`, verschlüsselt im Zwischenspeicher). Entschlüsselt wird nichts. Andere fehlende Zeichen kommen aus der Windows-Schrift.
- 🧾 `/party` fehlte, weil andere Namen am Listennamen erraten wurden und kleine Listen (20 Befehle, 8 davon vom Client) nicht als Client-Seite galten. Jetzt zählt der Verweis des Spiels; Befehle ohne Parameter färben nichts rot. Am Server lesend geprüft: `party`, `hub`, `lang`, `report`, `t`.
- ⚠️ Der Nutzer spielte dabei mit geladenen Plugins auf einem öffentlichen Server; auf das Bannrisiko wurde hingewiesen.

## Serversymbole im Chat (25.11.2026, Wunsch und Klarstellung des Nutzers)
- 🗂️ Minecraft führt Schrifttafeln in einem Verzeichnis unter lesbarem Namen (`font/glyph_E4`), mit Verweis auf das dauerhaft ausgepackte Bild samt Größe, Maßen und Format. `gamemem::sheets` geht genau diesen Weg und prüft jede Stufe; verschiedene Bilder für eine Tafel heißen keins. Auf Lifeboat dreimal nacheinander: `E1` und `E4` in etwa 170 ms, jederzeit auf dem Server.
- 🌐 Gilt für jeden Server und alle 256 Seiten (`glyph_00` bis `glyph_FF`). Reihenfolge je Zeichen: Minecrafts Schrift, dann Tafel des Servers, dann Windows; selbst belegte Zeichen immer von der Tafel. Nicht dabei: `default8` (ersetzt die Grundschrift, hier gilt der Java-Stil), Zeichen oberhalb U+FFFF und alle anderen Teile der Packs.
- 🧠 Nur im Arbeitsspeicher, im Menü verworfen, bis zu 32 Tafeln. Ohne Befehlsliste gilt keine Welt als offen.
- 🗑️ Verworfen: die PNG-Datei beim Beitreten abpassen (E4 lag teils kürzer als 120 ms vor, im ersten Versuch nur eine Lücke) und Minecrafts eigene Textausgabe (keine lesbaren Klassennamen).
- 🧪 `examples/tafeln` zeigt, welche Tafeln das laufende Spiel nennt. Test malt ohne Grafikkarte und prüft Bildpunkte. Im Chat noch nicht bestätigt.
