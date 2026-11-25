# Baut und packt, was Spieler bekommen: dist\MettClient-<Marke>.zip. Aufruf: pwsh tools/packen.ps1 -Version v0.1.0
param([Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+(-beta\.\d+)?$')][string]$Version)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$ziel = 'x86_64-pc-windows-msvc'
cargo build --release --locked --target $ziel -p mett-core -p plugin-hud -p plugin-chat -p plugin-chatui -p plugin-zoom -p mett-client
if ($LASTEXITCODE) { throw 'Bau fehlgeschlagen' }
$meta = cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json
$bau = Join-Path $meta.target_directory "$ziel\release"
$paket = 'dist\paket\MettClient'
if (Test-Path 'dist\paket') { Remove-Item 'dist\paket' -Recurse -Force }
New-Item -ItemType Directory -Force "$paket\plugins", "$paket\Lizenzen" | Out-Null
Copy-Item "$bau\MettClient.exe", "$bau\mett_core.dll", 'packaging\LIESMICH.txt' $paket
# Nur diese Plugins; Entwicklungs-Plugins werden nie ausgeliefert.
Copy-Item "$bau\mett_hud.dll" "$paket\plugins\hud.dll"
Copy-Item "$bau\chat.dll", "$bau\chatui.dll", "$bau\zoom.dll" "$paket\plugins"
Copy-Item 'assets\fonts\Archivo-OFL.txt' "$paket\Lizenzen\Archivo-OFL.txt"
Copy-Item 'assets\appleskin\LICENSE.txt' "$paket\Lizenzen\AppleSkin-Unlicense.txt"
Copy-Item 'assets\game-icons\LIZENZ.txt' "$paket\Lizenzen\game-icons.txt"
Copy-Item 'packaging\Rust-Bibliotheken.txt' "$paket\Lizenzen\Rust-Bibliotheken.txt"
$zip = "dist\MettClient-$Version.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $paket -DestinationPath $zip
Get-FileHash $zip | Format-List
