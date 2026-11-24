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
    [switch]$Complete,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
. (Join-Path $PSScriptRoot 'kit-download.ps1')
Add-Type -AssemblyName System.IO.Compression.FileSystem
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$cache = Join-Path $env:LOCALAPPDATA "DLSS5Forge\cache"
New-Item -ItemType Directory -Force -Path $Dir, $cache | Out-Null
$Dir = (Resolve-Path $Dir).Path

function Ok($t)   { Write-Host "      OK  $t" -ForegroundColor Green }
function Warn($t) { Write-Host "      !   $t" -ForegroundColor Yellow }
function Info($t) { Write-Host "      ..  $t" -ForegroundColor DarkGray }

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
$optiHash = if ($OptiTag -eq 'v0.2.0-dlssnr' -and $OptiAsset -eq 'OptiScaler-DLSSNR-v0.2.0.zip') {
    '8EECE7A4D7DE6DE5917F0C99AC60540B2D77022E7699BBA717B0A6D9E1829BCE'
}
if ($Complete) { Info '[1/4] OptiScaler e bibliotecas de upscaling e Frame Generation' }
$optiZip = Get-KitAsset `
    "https://github.com/Dagherbou/OptiScaler_DLSSNR/releases/download/$OptiTag/$OptiAsset" `
    (Join-Path $cache "optiscaler-dlssnr-$OptiTag.zip") $optiHash -Force:$Force

$feederTag = 'v0.8.93'
$feederAsset = "OptiScaler-NR-$feederTag.zip"
$feederHash = '8003065D88813B342049B869E8C894F8771C58F3D1A630DDF42FABD5C9D2F5FC'
$feederZip = Get-KitAsset `
    "https://github.com/jlrouzies-fr/OptiScaler-DLSSNR-PreSR-Multipass/releases/download/$feederTag/$feederAsset" `
    (Join-Path $cache $feederAsset) $feederHash -Force:$Force
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
    if (-not (Extract-Entry $optiZip $m.entry $dest)) { throw "Arquivo ausente no kit OptiScaler: $($m.entry)" }
    $i = Get-Item $dest
    $rows += [pscustomobject]@{
        Arquivo = $m.dest
        MB      = [math]::Round($i.Length / 1MB, 1)
        Origem  = 'OptiScaler DLSSNR'
        Assinatura = (Sig $dest)
    }
}

if ($WithStreamline -or $Complete) {
    if ($Complete) { Info '[2/4] NVIDIA Streamline e DLSS Frame Generation' }
    & (Join-Path $PSScriptRoot 'fetch-runtime.ps1') -Dir (Split-Path $Dir -Parent) -Force:$Force
}

$modelSource = 'Local NVIDIA model'
if ($Complete) {
    Info '[3/4] Modelo NVIDIA DLSS Neural Rendering'
    if ([IO.File]::Exists($modelDest) -and (Get-Item -LiteralPath $modelDest).Length -ge 150MB -and
        (Sig $modelDest) -eq 'NVIDIA valida') {
        Ok 'Modelo NVIDIA assinado já disponível.'
    } else {
        $modelSource = 'RankFTW/rhi-repo dlssnr-310.9.16-nvidia'
        $modelZip = Get-KitAsset `
            'https://github.com/RankFTW/rhi-repo/releases/download/dlssnr-310.9.16-nvidia/nvngx_dlssnr_310.9.16-nvidia.zip' `
            (Join-Path $cache 'nvngx_dlssnr-dlssnr-310.9.16-nvidia.zip') `
            '5277355F0578D36F2A25413D8C7FF4C6F9966CEA5BAD714305F23B20173E09F6' -Force:$Force
        $pendingModel = "$modelDest.$PID.dll"
        try {
            if (-not (Extract-Entry $modelZip 'nvngx_dlssnr.dll' $pendingModel) -or
                (Get-Item -LiteralPath $pendingModel).Length -lt 150MB -or (Sig $pendingModel) -ne 'NVIDIA valida') {
                throw 'O modelo baixado não passou na verificação da assinatura NVIDIA.'
            }
            Move-Item -LiteralPath $pendingModel -Destination $modelDest -Force
        } finally {
            if ([IO.File]::Exists($pendingModel)) { Remove-Item -LiteralPath $pendingModel -Force }
        }
        Ok 'Modelo NVIDIA verificado e instalado.'
    }

    Info '[4/4] DLSS5oneclick'
    $oneClickPath = Join-Path (Split-Path $Dir -Parent) 'tools\dlss5oneclick.exe'
    $oneClick = Get-KitAsset `
        'https://github.com/faisalkindi/DLSS5oneclick/releases/download/v0.14.37/dlss5oneclick.exe' `
        $oneClickPath '557A50F83D3BDCBAF91B54AFC141345822896DCD2463BE22AFF40EA60F9B25EC' -Force:$Force
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
        fonte = $modelSource
        dll   = 'nvngx_dlssnr.dll'
    }
    arquivos = $rows
    oneclick = $(if ($Complete) { @{ versao = 'v0.14.37'; arquivo = $oneClick } })
    requer = "RTX 50 + jogo que ja usa DLSS (DX11/DX12/Vulkan)"
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $Dir "manifest.json") -Encoding UTF8
Ok "manifesto: $(Join-Path $Dir 'manifest.json')"

Write-Host ""
if ($Complete) { Ok 'Kit completo. Selecione um jogo para ver a prévia da instalação.' }
Write-Host ""
Write-Host ""
