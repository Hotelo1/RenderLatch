# Diagnostico: procura instalacoes do Euro Truck Simulator 2 e verifica se
# ja tem DLSS nativo / nossos arquivos instalados.

$roots = @(
    "C:\Program Files\Euro Truck Simulator 2",
    "D:\SteamLibrary\steamapps\common\Euro Truck Simulator 2",
    "C:\Games\Euro Truck Simulator 2",
    "E:\SteamLibrary\steamapps\common\Euro Truck Simulator 2"
)

$found = Get-ChildItem "C:\" -Directory -Recurse -ErrorAction SilentlyContinue 2>$null |
    Where-Object { $_.FullName.ToLower().Contains("euro truck simulator 2") } |
    Select-Object -First 8 | ForEach-Object { $_.FullName }

if ($found) { $roots += $found }

$ok = 0
foreach ($r in $roots) {
    if (-not (Test-Path $r)) { continue }
    $ok += 1
    Write-Output "ACHADO: $r"
    $b = Get-ChildItem $r -Recurse -Filter _dlss5forge -ErrorAction SilentlyContinue
    if ($b) { $b | ForEach-Object { "   _dlss5forge: $($_.FullName)" } }
    $dl = Get-ChildItem $r -Recurse -Filter nvngx_dlss*.dll -ErrorAction SilentlyContinue
    if ($dl) { $dl | ForEach-Object { "   nvngx_dlss.dll: $($_.FullName)" } }
    $sl = Get-ChildItem $r -Recurse -Filter sl.interposer.dll -ErrorAction SilentlyContinue
    if ($sl) { $sl | ForEach-Object { "   sl.interposer.dll: $($_.FullName)" } }
}
if ($ok -eq 0) { Write-Output "nenhuma pasta de ETS2 encontrada nos locais comuns" }
