# Eine Testnachricht in genau einer erlaubten Welt. Bricht bei jeder Unklarheit ab.
param([Parameter(Mandatory)][string]$Funktion, [Parameter(Mandatory)][string]$Modell)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$bin = "$env:LOCALAPPDATA\MettGaming\build\mett-client\release\examples"
try {
  $games = @(Get-Process Minecraft.Windows -ErrorAction SilentlyContinue)
  if ($games.Count -ne 1) { throw "Es muss genau ein Minecraft laufen, es sind $($games.Count)." }
  $g = $games[0]; $null = $g.Handle; $start = [datetime]$g.StartTime
  $worlds = @(Get-ChildItem "$env:APPDATA\Minecraft Bedrock\Users" -Directory | ForEach-Object { Join-Path $_.FullName 'games\com.mojang\minecraftWorlds' } | Where-Object { Test-Path $_ } | ForEach-Object { Get-ChildItem $_ -Directory })
  $used = @($worlds | Where-Object { @(Get-ChildItem $_.FullName -Recurse -File | Where-Object { $_.LastWriteTime -gt $start }).Count -gt 0 } | ForEach-Object Name)
  if ($used.Count -ne 1 -or $used[0] -ne 'MettDebug1') { throw "Erwartet nur MettDebug1, benutzt: $($used -join ', ')" }
  $scan = (& "$bin\finden.exe") -join ' '
  if (@([regex]::Matches($scan, 'Liste bei')).Count -ne 1) { throw "Chatliste nicht eindeutig: $scan" }
} catch { "ABBRUCH, nichts geladen: $($_.Exception.Message)"; exit 1 }
$ErrorActionPreference = 'Continue'
"vorher: $scan"
& "$bin\sendetest.exe" $g.Id "$root\dist\loadertest" $Funktion $Modell
Start-Sleep 3; $g.Refresh()
"nachher: $((& "$bin\finden.exe") -join ' ')"
"lebt=$(-not $g.HasExited) reagiert=$($g.Responding)"
"Absturzmeldungen: " + @(Get-WinEvent -FilterHashtable @{LogName = 'Application'; ProviderName = 'Application Error'; StartTime = (Get-Date).AddMinutes(-2) } -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'Minecraft' }).Count
