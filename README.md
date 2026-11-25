# Mett-Client
HUD, Menü und Chat direkt in Minecraft Bedrock Edition unter Windows. Inoffiziell, nicht mit Mojang oder Microsoft verbunden.

- `crates/mett-client` Programm, `plugin-hud` Anzeige und Eingabe im Spiel, `hudlink` Verbindung, `render` Direct2D, `hud` Layout, `sources` Eingaben, `config` Einstellungen
- `assets/fonts` Archivo (SIL OFL, Lizenz liegt bei)
- `assets/appleskin` Symbole aus AppleSkin (Unlicense, Lizenz liegt bei)
- `assets/game-icons` Symbole von game-icons.net (CC BY 3.0, Urheber liegen bei)
- `crates/compat` Update-Schutz je Funktion, `crates/gamemem` Lesezugriff

Gedacht für den Start über den Mett Launcher (`--launcher`); ohne ihn beendet sich das Programm mit einem Hinweis.
Entwicklung: `-- --dev` startet ohne Anzeige, weil dann nichts ins Spiel geladen wird.
Ende-zu-Ende ohne Minecraft: `cargo build --release -p plugin-hud --examples`, dann `durchlauf <Ordner>`.

Veröffentlichen: Marke `vX.Y.Z` hochladen, GitHub baut mit `tools/packen.ps1` das Zip. Spieler bekommen es über den Mett Launcher (Katalog: `Mett-Gaming/launcher-catalog`).
