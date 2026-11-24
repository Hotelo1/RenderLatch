<#
  fetch-runtime.ps1 - baixa o runtime OFICIAL da NVIDIA para .\runtime

  Fontes (somente repositorios oficiais da NVIDIA):
    https://github.com/NVIDIA-RTX/Streamline/releases

  O que ele NAO faz: baixar nvngx_dlssnr.dll / sl.dlss_nr.dll de espelho
  nao oficial. Esses dois arquivos nao existem em nenhum release publico da
  NVIDIA - so acompanham jogos que implementaram DLSS 5. Use -FromGame para
  importar de uma pasta de jogo que voce possui.

  Uso:
    .\tools\fetch-runtime.ps1
    .\tools\fetch-runtime.ps1 -FromGame "D:\...\NBA2K27"
    .\tools\fetch-runtime.ps1 -Force
#>
param(
    [string]$Dir = (Join-Path $PSScriptRoot "..\runtime"),
    [string]$Streamline = "2.14.16",
    [string]$FromGame = "",
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$cache = Join-Path $env:LOCALAPPDATA "DLSS5Forge\cache"
New-Item -ItemType Directory -Force -Path $Dir, $cache | Out-Null
$Dir = (Resolve-Path $Dir).Path

function Ok($t)   { Write-Host "      OK  $t" -ForegroundColor Green }
function Warn($t) { Write-Host "      !   $t" -ForegroundColor Yellow }
function Info($t) { Write-Host "      ..  $t" -ForegroundColor DarkGray }

function Get-Zip([string]$version) {
    $name = "streamline-sdk-v$version.zip"
    $path = Join-Path $cache $name
    if ((Test-Path $path) -and -not $Force) {
        Info "cache: $name"
        return $path
    }
    $url = "https://github.com/NVIDIA-RTX/Streamline/releases/download/v$version/$name"
    Info "baixando $name (~260 MB)"
    $pending = "$path.$PID.download"
    try {
        curl.exe --fail --location --silent --show-error --output $pending $url
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $pending)) { throw "download falhou: $url" }
        Move-Item -LiteralPath $pending -Destination $path -Force
    } finally {
        if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending -Force }
    }
    Ok ("{0} ({1:N0} MB)" -f $name, ((Get-Item $path).Length / 1MB))
    return $path
}

function Extract-Entry([string]$zipPath, [string]$entry, [string]$dest) {
    $z = [IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $e = $z.GetEntry($entry)
        if (-not $e) { return $false }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($e, $dest, $true)
        return $true
    } finally { $z.Dispose() }
}

function Check-Sig([string]$p) {
    $s = Get-AuthenticodeSignature -FilePath $p
    $signer = ""
    if ($s.SignerCertificate) { $signer = $s.SignerCertificate.Subject }
    [pscustomobject]@{
        Status = $s.Status.ToString()
        Signer = $signer
        Nvidia = ($signer -match 'NVIDIA')
        Valid  = ($s.Status -eq 'Valid' -and $signer -match 'NVIDIA')
    }
}

Write-Host ""
Write-Host "  RenderLatch - NVIDIA Streamline" -ForegroundColor White
Write-Host "  ============================================================" -ForegroundColor DarkGray
Write-Host "  destino: $Dir"
Write-Host ""

# ---------------------------------------------------------------- Streamline
$zip = Get-Zip $Streamline
if ($Streamline -eq '2.14.16' -and
    (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne '32041E51DB0CAB709A271B72A352281B690C2ABBD669A827809401A5DD23956B') {
    throw 'SHA-256 inesperado no Streamline SDK. Atualize o cache com -Force.'
}

$core = @(
    'sl.common.dll', 'sl.dlss.dll', 'sl.dlss_g.dll', 'sl.dlss_d.dll',
    'sl.interposer.dll', 'sl.nis.dll', 'sl.pcl.dll', 'sl.reflex.dll',
    'nvngx_dlss.dll', 'nvngx_dlssd.dll', 'nvngx_dlssg.dll'
)

$rows = @()
foreach ($f in $core) {
    $dest = Join-Path $Dir $f
    if (-not (Extract-Entry $zip "bin/x64/$f" $dest)) { Warn "ausente no zip: $f"; continue }
    $sig = Check-Sig $dest
    $ver = (Get-Item $dest).VersionInfo.FileVersion
    $rows += [pscustomobject]@{
        Arquivo = $f
        Versao  = if ($ver) { $ver } else { "?" }
        MB      = [math]::Round((Get-Item $dest).Length / 1MB, 1)
        Assinatura = if ($sig.Valid) { "NVIDIA valida" } else { "INVALIDA ($($sig.Status))" }
        Sha256  = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLower()
    }
}

# ---------------------------------------------------------------- DLSS 5 (jogo)
$nr = @('nvngx_dlssnr.dll', 'sl.dlss_nr.dll')
if ($FromGame) {
    Write-Host ""
    Write-Host "  Importando DLSS 5 de: $FromGame" -ForegroundColor White
    $src = @{}
    Get-ChildItem -LiteralPath $FromGame -Recurse -File -Include $nr -ErrorAction SilentlyContinue |
        ForEach-Object { if (-not $src.ContainsKey($_.Name)) { $src[$_.Name] = $_.FullName } }

    foreach ($f in $nr) {
        if (-not $src.ContainsKey($f)) { Warn "nao encontrado no jogo: $f"; continue }
        $dest = Join-Path $Dir $f
        Copy-Item $src[$f] $dest -Force
        $sig = Check-Sig $dest
        $ver = (Get-Item $dest).VersionInfo.FileVersion
        $rows += [pscustomobject]@{
            Arquivo = $f
            Versao  = if ($ver) { $ver } else { "?" }
            MB      = [math]::Round((Get-Item $dest).Length / 1MB, 1)
            Assinatura = if ($sig.Valid) { "NVIDIA valida" } else { "INVALIDA ($($sig.Status))" }
            Sha256  = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLower()
        }
    }
}

# ---------------------------------------------------------------- relatorio
Write-Host ""
$rows | Sort-Object Arquivo | Format-Table -AutoSize

$bad = @($rows | Where-Object { $_.Assinatura -notlike 'NVIDIA valida*' })
if ($bad.Count -gt 0) {
    Write-Host "  ATENCAO: $($bad.Count) arquivo(s) sem assinatura NVIDIA valida." -ForegroundColor Red
}

$manifest = [pscustomobject]@{
    gerado_em    = (Get-Date).ToString("s")
    fonte        = "github.com/NVIDIA-RTX/Streamline v$Streamline"
    destino      = $Dir
    arquivos     = $rows
    dlss5_modelo = if (Test-Path (Join-Path $Dir 'nvngx_dlssnr.dll')) { "presente" } else { "ausente" }
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $Dir "manifest.json") -Encoding UTF8
Ok "manifesto: $(Join-Path $Dir 'manifest.json')"

if ($manifest.dlss5_modelo -eq 'ausente') {
    Write-Host ""
    Write-Host "  DLSS 5 (modelo) nao veio: nvngx_dlssnr.dll e sl.dlss_nr.dll nao" -ForegroundColor Yellow
    Write-Host "  existem em nenhum release publico da NVIDIA. Eles acompanham os" -ForegroundColor Yellow
    Write-Host "  jogos que implementaram DLSS 5 (ex.: NBA 2K27, Hogwarts Legacy)." -ForegroundColor Yellow
    Write-Host "  Rode de novo apontando para a pasta do jogo:" -ForegroundColor Yellow
    Write-Host "    .\tools\fetch-runtime.ps1 -FromGame `"D:\...\Jogo`"" -ForegroundColor Gray
}
Write-Host ""
