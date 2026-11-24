<#
  fetch-dlss5.ps1 - monta o kit de DLSS 5 em .\runtime\dlss5

  O kit tem 4 pecas:
    OptiScaler.dll              o gancho (fork DLSSNR do Dagherbou)
    nvngx.dll_dlssnr.dll        snippet que o OptiScaler exige
    nvngx_dlssnr.dll            o MODELO da DLSS 5 (~254 MB)
    OptiScaler.ini              lido e ajustado por maquina no install

  Importe o modelo NVIDIA de uma instalacao compativel com -Model.

  Uso:
    .\tools\fetch-dlss5.ps1
    .\tools\fetch-dlss5.ps1 -Model "D:\...\nvngx_dlssnr.dll"
    .\tools\fetch-dlss5.ps1 -Force
#>
param(
    [string]$Dir = (Join-Path $PSScriptRoot "..\runtime\dlss5"),
    [string]$OptiTag = "v0.2.0-dlssnr",
    [string]$OptiAsset = "OptiScaler-DLSSNR-v0.2.0.zip",
    [string]$Model = "",
    [switch]$ImportOnly,
    [switch]$WithStreamline,
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

function Get-Zip([string]$url, [string]$name) {
    $path = Join-Path $cache $name
    if ((Test-Path $path) -and -not $Force) {
        Info "cache: $name"
        return $path
    }
    Info "baixando $name"
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
        $p = Split-Path $dest -Parent
        if ($p -and -not (Test-Path $p)) { New-Item -ItemType Directory -Force -Path $p | Out-Null }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($e, $dest, $true)
        return $true
    } finally { $z.Dispose() }
}

function Sig([string]$p) {
    $s = Get-AuthenticodeSignature -LiteralPath $p
    $signer = ""
    if ($s.SignerCertificate) { $signer = $s.SignerCertificate.Subject }
    if ($s.Status -eq 'Valid' -and $signer -match 'NVIDIA') { return "NVIDIA valida" }
    if ($s.Status -eq 'Valid') { return "valida ($(($signer -split ',')[0]))" }
    return "sem assinatura ($($s.Status))"
}

Write-Host ""
$modelDest = Join-Path $Dir 'nvngx_dlssnr.dll'
if ($Model) {
    $source = Get-Item -LiteralPath $Model
    if ($source -isnot [IO.FileInfo] -or $source.Name -ine 'nvngx_dlssnr.dll' -or $source.Length -lt 150MB -or
        (Sig $source.FullName) -ne 'NVIDIA valida') {
        throw 'Selecione um nvngx_dlssnr.dll com assinatura NVIDIA válida e pelo menos 150 MB.'
    }
    if ($source.FullName -ine $modelDest) {
        Copy-Item -LiteralPath $source.FullName -Destination $modelDest -Force
    }
    Ok 'Modelo NVIDIA importado.'
} elseif ($ImportOnly) {
    throw 'Informe o caminho do modelo com -Model.'
}
if ($ImportOnly) { return }

Write-Host "  RenderLatch - kit de DLSS Neural Rendering" -ForegroundColor White
Write-Host "  ============================================================" -ForegroundColor DarkGray
Write-Host "  destino: $Dir"
Write-Host ""

# ------------------------------------------------------------------ OptiScaler
$optiZip = Get-Zip `
    "https://github.com/Dagherbou/OptiScaler_DLSSNR/releases/download/$OptiTag/$OptiAsset" `
    "optiscaler-dlssnr-$OptiTag.zip"
if ($OptiTag -eq 'v0.2.0-dlssnr' -and $OptiAsset -eq 'OptiScaler-DLSSNR-v0.2.0.zip' -and
    (Get-FileHash -LiteralPath $optiZip -Algorithm SHA256).Hash -ne '8EECE7A4D7DE6DE5917F0C99AC60540B2D77022E7699BBA717B0A6D9E1829BCE') {
    throw 'SHA-256 inesperado no kit OptiScaler. Atualize o cache com -Force.'
}

$feederTag = 'v0.8.93'
$feederAsset = "OptiScaler-NR-$feederTag.zip"
$feederZip = Get-Zip `
    "https://github.com/jlrouzies-fr/OptiScaler-DLSSNR-PreSR-Multipass/releases/download/$feederTag/$feederAsset" `
    $feederAsset
$feederHash = '8003065D88813B342049B869E8C894F8771C58F3D1A630DDF42FABD5C9D2F5FC'
if ((Get-FileHash -LiteralPath $feederZip -Algorithm SHA256).Hash -ne $feederHash) {
    throw "SHA-256 inesperado: $feederAsset. Tente atualizar o kit com -Force."
}
foreach ($entry in @('OptiScaler.dll', 'OptiScaler.ini')) {
    if (-not (Extract-Entry $feederZip $entry (Join-Path $Dir "feeder\$entry"))) {
        throw "Arquivo ausente no kit Feeder: $entry"
    }
}

$map = @(
    @{ entry = 'OptiScaler.dll';                              dest = 'OptiScaler.dll' },
    @{ entry = 'nvngx.dll_dlssnr.dll';                        dest = 'nvngx.dll_dlssnr.dll' },
    @{ entry = 'OptiScaler.ini';                              dest = 'OptiScaler.ini' },
    @{ entry = 'OptiScaler\amd_fidelityfx_loader_dx12.dll'; dest = 'OptiScaler\amd_fidelityfx_loader_dx12.dll' },
    @{ entry = 'OptiScaler\amd_fidelityfx_upscaler_dx12.dll'; dest = 'OptiScaler\amd_fidelityfx_upscaler_dx12.dll' },
    @{ entry = 'OptiScaler\amd_fidelityfx_framegeneration_dx12.dll'; dest = 'OptiScaler\amd_fidelityfx_framegeneration_dx12.dll' },
    @{ entry = 'OptiScaler\libxess.dll'; dest = 'OptiScaler\libxess.dll' },
    @{ entry = 'OptiScaler\libxess_fg.dll'; dest = 'OptiScaler\libxess_fg.dll' },
    @{ entry = 'OptiScaler\libxell.dll'; dest = 'OptiScaler\libxell.dll' },
    @{ entry = 'OptiScaler\D3D12_OptiScaler\D3D12Core.dll';   dest = 'D3D12_OptiScaler\D3D12Core.dll' },
    @{ entry = 'Licenses\RenoDX_ATTRIBUTION.txt';             dest = 'Licenses\RenoDX_ATTRIBUTION.txt' }
)

$rows = @()
foreach ($m in $map) {
    $dest = Join-Path $Dir $m.dest
    if (-not (Extract-Entry $optiZip $m.entry $dest)) { Warn "ausente no zip: $($m.entry)"; continue }
    $i = Get-Item $dest
    $rows += [pscustomobject]@{
        Arquivo = $m.dest
        MB      = [math]::Round($i.Length / 1MB, 1)
        Origem  = 'OptiScaler DLSSNR'
        Assinatura = (Sig $dest)
    }
}

if ($WithStreamline) {
    & (Join-Path $PSScriptRoot 'fetch-runtime.ps1') -Dir (Split-Path $Dir -Parent) -Force:$Force
}

if (Test-Path $modelDest) {
    $i = Get-Item $modelDest
    $rows += [pscustomobject]@{
        Arquivo = 'nvngx_dlssnr.dll'
        MB      = [math]::Round($i.Length / 1MB, 1)
        Origem  = 'modelo DLSS 5'
        Assinatura = (Sig $modelDest)
    }
}

# ------------------------------------------------------------------ relatorio
Write-Host ""
$rows | Sort-Object Arquivo | Format-Table -AutoSize

$modelMb = 0
if (Test-Path $modelDest) { $modelMb = [math]::Round((Get-Item $modelDest).Length / 1MB, 0) }
if ($modelMb -lt 150) {
    Warn 'Importe o nvngx_dlssnr.dll na interface para usar o OptiScaler DLSS-NR.'
}

$manifest = [pscustomobject]@{
    gerado_em = (Get-Date).ToString("s")
    optiscaler = [pscustomobject]@{
        fonte = "github.com/Dagherbou/OptiScaler_DLSSNR $OptiTag"
        dll   = 'OptiScaler.dll'
    }
    optiscaler_feeder = [pscustomobject]@{
        fonte = "github.com/jlrouzies-fr/OptiScaler-DLSSNR-PreSR-Multipass $feederTag"
        dll   = 'feeder\OptiScaler.dll'
        sha256_zip = $feederHash
    }
    modelo = [pscustomobject]@{
        fonte = 'Local NVIDIA model'
        dll   = 'nvngx_dlssnr.dll'
    }
    arquivos = $rows
    requer = "RTX 50 + jogo que ja usa DLSS (DX11/DX12/Vulkan)"
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $Dir "manifest.json") -Encoding UTF8
Ok "manifesto: $(Join-Path $Dir 'manifest.json')"

Write-Host ""
Write-Host "  Agora aplique num jogo:" -ForegroundColor White
Write-Host "    .\forge.exe dlss5 --game `"D:\...\Jogo`"" -ForegroundColor Gray
Write-Host ""
Write-Host ""
