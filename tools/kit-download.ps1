# A portable launch can inherit PowerShell 7 module paths. Load the Windows
# PowerShell implementations used for file hashes and Authenticode checks.
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -ErrorAction Stop

function Get-KitAsset([string]$Url, [string]$Path, [string]$Sha256, [switch]$Force) {
    if ([IO.File]::Exists($Path) -and -not $Force) {
        if (-not $Sha256 -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Sha256) {
            Write-Host "      OK  Já disponível: $([IO.Path]::GetFileName($Path))"
            return $Path
        }
        Write-Host "      ..  Substituindo o arquivo com checksum incorreto: $([IO.Path]::GetFileName($Path))"
    }

    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))
    $pending = "$Path.$PID.download"
    Write-Host "      ..  Baixando $([IO.Path]::GetFileName($Path))"
    try {
        $curl = Join-Path ([Environment]::SystemDirectory) 'curl.exe'
        & $curl --fail --location --silent --show-error --retry 2 --connect-timeout 20 --max-time 900 --output $pending $Url
        if ($LASTEXITCODE -ne 0 -or -not [IO.File]::Exists($pending)) {
            throw "Falha no download de $([IO.Path]::GetFileName($Path)). Verifique sua conexão e tente novamente."
        }
        if ($Sha256 -and (Get-FileHash -LiteralPath $pending -Algorithm SHA256).Hash -ne $Sha256) {
            throw "SHA-256 inesperado para $([IO.Path]::GetFileName($Path))."
        }
        if ([IO.File]::Exists($Path)) {
            [IO.File]::Replace($pending, $Path, "$pending.previous")
        } else {
            [IO.File]::Move($pending, $Path)
        }
    } finally {
        foreach ($file in @($pending, "$pending.previous")) {
            if ([IO.File]::Exists($file)) { Remove-Item -LiteralPath $file -Force }
        }
    }
    Write-Host "      OK  $([IO.Path]::GetFileName($Path))"
    return $Path
}
