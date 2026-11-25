# Laedt ein Pruef-Plugin fuer fuenf Sekunden ins Spiel. Bricht bei jeder Unklarheit ab (nie "im Zweifel laden").
# Aufruf: spieltest.ps1 -Plugin debug [-Welt MettDebug1]
#   ohne -Welt: nur im Hauptmenue; mit -Welt: nur, wenn genau diese Welt seit dem Spielstart benutzt wurde.
param([Parameter(Mandatory)][string]$Plugin, [string]$Welt)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$bin = "$env:LOCALAPPDATA\MettGaming\build\mett-client\release\examples"
try {
  $games = @(Get-Process Minecraft.Windows -ErrorAction SilentlyContinue)
  if ($games.Count -ne 1) { throw "Es muss genau ein Minecraft laufen, es sind $($games.Count)." }
  $g = $games[0]; $null = $g.Handle; $start = [datetime]$g.StartTime
  $worlds = @(Get-ChildItem "$env:APPDATA\Minecraft Bedrock\Users" -Directory | ForEach-Object { Join-Path $_.FullName 'games\com.mojang\minecraftWorlds' } | Where-Object { Test-Path $_ } | ForEach-Object { Get-ChildItem $_ -Directory })
  if ($worlds.Count -eq 0) { throw 'Weltordner nicht gefunden.' }
  $used = @($worlds | Where-Object { @(Get-ChildItem $_.FullName -Recurse -File | Where-Object { $_.LastWriteTime -gt $start }).Count -gt 0 } | ForEach-Object Name)
  $scan = (& "$bin\finden.exe") -join ' '
  if ($LASTEXITCODE -ne 0 -or $scan -notmatch 'Dauer') { throw 'Chat-Pruefung lieferte kein Ergebnis.' }
  if ($Welt) {
    if ($used.Count -ne 1 -or $used[0] -ne $Welt) { throw "Erwartet nur $Welt, benutzt: $($used -join ', ')" }
  } elseif ($used.Count -gt 0 -or $scan -match 'Liste bei') { throw "Welt offen oder benutzt: $($used -join ', ')" }
} catch { "ABBRUCH, nichts geladen: $($_.Exception.Message)"; exit 1 }
$ErrorActionPreference = 'Continue'
& "$bin\spieltest.exe" $g.Id "$root\dist\loadertest" $Plugin
Start-Sleep 5; $g.Refresh()
"lebt=$(-not $g.HasExited) reagiert=$($g.Responding)"
"Absturzmeldungen: " + @(Get-WinEvent -FilterHashtable @{LogName = 'Application'; ProviderName = 'Application Error'; StartTime = (Get-Date).AddMinutes(-2) } -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'Minecraft|MettClient' }).Count
