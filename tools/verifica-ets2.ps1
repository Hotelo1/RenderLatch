# Script de diagnóstico: por deploy e pifia do OptiScaler, a partir da cópia do jogo.
# Rodar:  powershell -ExecutionPolicy Bypass -File .\tools\verifica-ets2.ps1

$name = "Euro Truck Simulator 2"
$roots = @("C:\Program Files (x86)\Steam\steamapps\common\$name",
           "D:\SteamLibrary\steamapps\common\$name",
           "E:\SteamLibrary\steamapps\common\$name")

Write-Output "=== achando instalação ==="
$g = $null
foreach ($r in $roots) { if (Test-Path $r) { $g = $r } }
if (-not $g) {
    Write-Output "Pasta não encontrada nos comuns. Passa o caminho da última instalação (a que tem _dlss5forge):"
    Read-Host "Pasta do ETS2 (ex.: ...\bin\win_x64)"
}

$exe = Get-ChildItem $g -Recurse -Filter win_x64.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $exe) { Write-Output "Nenhum exe win_x64.exe achado em $g"; exit }
$exe = $exe.FullName
Write-Output "exe: $exe"

Write-Output "=== O que o OptiScaler colocou junto do exe ==="
$dir = Split-Path $exe
Get-ChildItem $dir -File | ForEach-Object {
    $kb = [math]::Round($_.Length/1KB)
    Write-Output "  $($_.Name)  ($kb KB)"
}

Write-Output "=== dxgi.dll = OptiScaler (renomeado) ==="
$d = Join-Path $dir "dxgi.dll"
if (Test-Path $d) { Get-AuthenticodeSignature -LiteralPath $d | Select-Object -Property Status }
else { Write-Output "  !!! dxgi.dll NÃO existe em $dir" }

Write-Output "=== OptiScaler.log (últimos 40 linhas) ==="
$log = Join-Path $dir "OptiScaler.log"; $log2 = Join-Path $dir "OptiScaler.txt"
$ln = if (Test-Path $log) { Get-Content -LiteralPath $log -Tail 40 } elseif (Test-Path $log2) { Get-Content -LiteralPath $log2 -Tail 40 }
$ln = "$($ln)"
if ($ln -match 'Dep[Johnqk]:|\[OptiScaler') { $ln | ForEach-Object { Write-Output $_ } }
else { Write-Output "   (sem OptiScaler.log em $dir -- se abrir o jogo de nada, um ponto criticalo.)" }

Write-Output "=== GpuPreference (GPU de alto desempenho) ==="
reg query HKCU\Software\Microsoft\DirectX\UserGpuPreferences 2>&1 | Select-String ($exe)

Write-Output "=== Confirmar antes de aplicar: destino do jogo ==="
cd "$PSScriptRoot\.."
.\forge.exe dlss5 --game $g --dry-run | Select-String -Pattern "jogo|destino|exe|pronto|nvngx_dlss"
