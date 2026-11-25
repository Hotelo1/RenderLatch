param(
    [Parameter(Mandatory=$true)][ValidateSet('Check','Stage','Replace')][string]$Action,
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][version]$CurrentVersion,
    [string]$ReleaseTag,
    [string]$StagedFile,
    [string]$ExpectedHash,
    [int]$GuiProcessId
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$releases = 'https://github.com/Hotelo1/RenderLatch/releases'
$assetName = 'RenderLatch-windows-x64.exe'
$Executable = [IO.Path]::GetFullPath($Executable)
$folder = Split-Path -Parent $Executable

function Get-ReleaseUpdate {
    $response = Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 15 -Uri "$releases/latest"
    $releaseUrl = $response.BaseResponse.ResponseUri.AbsoluteUri
    if ($releaseUrl -cnotmatch ('^' + [regex]::Escape($releases) + '/tag/(v(\d+\.\d+\.\d+))$')) {
        throw 'A release mais recente não tem uma versão estável reconhecida.'
    }
    $tag = $Matches[1]
    $version = [version]$Matches[2]
    if ($version -le $CurrentVersion) { return }

    $downloadUrl = "$releases/download/$tag/$assetName"
    $checksum = Invoke-WebRequest -UseBasicParsing -TimeoutSec 15 -Uri "$downloadUrl.sha256"
    $content = $checksum.Content
    if ($content -is [byte[]]) { $content = [Text.Encoding]::UTF8.GetString($content) }
    if ($content -notmatch ('^\s*([0-9a-f]{64})[ \t]+\*?' + [regex]::Escape($assetName) + '\s*$')) {
        throw "O checksum publicado para $tag é inválido."
    }
    [pscustomobject]@{ Tag = $tag; Version = $version; Url = $downloadUrl; Hash = $Matches[1] }
}

function Get-RenderLatchVersion([string]$File) {
    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($File)
    if ($info.ProductName -cne 'RenderLatch' -or $info.ProductVersion -notmatch '^\d+\.\d+\.\d+$') {
        throw 'O arquivo não contém uma versão válida do RenderLatch.'
    }
    [version]$info.ProductVersion
}

try {
    if ($Action -eq 'Check') {
        $update = Get-ReleaseUpdate
        if ($update) { Write-Output $update.Tag }
        exit 0
    }

    if ($Action -eq 'Stage') {
        $update = Get-ReleaseUpdate
        if (!$update -or $update.Tag -cne $ReleaseTag) {
            throw 'A versão publicada mudou. Verifique as atualizações novamente.'
        }
        $pending = Join-Path $folder ('.renderlatch-update-' + [guid]::NewGuid().ToString('N') + '.exe')
        try {
            Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri $update.Url -OutFile $pending
            if ((Get-FileHash -LiteralPath $pending -Algorithm SHA256).Hash -ne $update.Hash -or
                (Get-RenderLatchVersion $pending) -ne $update.Version) {
                throw 'A verificação do executável baixado falhou.'
            }
            @{ Path = $pending; Hash = $update.Hash } | ConvertTo-Json -Compress
        } catch {
            if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending -Force }
            throw
        }
        exit 0
    }

    $StagedFile = [IO.Path]::GetFullPath($StagedFile)
    if ((Split-Path -Parent $StagedFile) -ne $folder -or
        (Split-Path -Leaf $StagedFile) -notmatch '^\.renderlatch-update-[0-9a-f]{32}\.exe$' -or
        $ExpectedHash -notmatch '^[0-9a-f]{64}$' -or $GuiProcessId -le 0 -or
        $ReleaseTag -notmatch '^v(\d+\.\d+\.\d+)$') {
        throw 'Atualização preparada inválida.'
    }
    $newVersion = [version]$Matches[1]
    try {
        $gui = Get-Process -Id $GuiProcessId -ErrorAction SilentlyContinue
        if ($gui -and !$gui.WaitForExit(30000)) { throw 'O RenderLatch ainda está aberto.' }

        $mutex = [Threading.Mutex]::new($false, 'Local\RenderLatch-Update')
        $locked = $false
        try {
            try { $locked = $mutex.WaitOne(10000) } catch [Threading.AbandonedMutexException] { $locked = $true }
            if (!$locked) { throw 'Já existe outra atualização em andamento.' }
            if ((Get-RenderLatchVersion $Executable) -ne $CurrentVersion -or $newVersion -le $CurrentVersion -or
                (Get-FileHash -LiteralPath $StagedFile -Algorithm SHA256).Hash -ne $ExpectedHash -or
                (Get-RenderLatchVersion $StagedFile) -ne $newVersion) {
                throw 'O executável atual ou a atualização mudou desde a verificação.'
            }

            $backup = Join-Path $folder ('.renderlatch-backup-' + [guid]::NewGuid().ToString('N') + '.bak')
            [IO.File]::Replace($StagedFile, $Executable, $backup, $false)
            try {
                Start-Process -FilePath $Executable -WorkingDirectory $folder -WindowStyle Hidden
            } catch {
                [IO.File]::Replace($backup, $Executable, $null, $false)
                throw
            }
            Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
        } finally {
            if ($locked) { $mutex.ReleaseMutex() }
            $mutex.Dispose()
        }
    } finally {
        if (Test-Path -LiteralPath $StagedFile) { Remove-Item -LiteralPath $StagedFile -Force -ErrorAction SilentlyContinue }
    }
} catch {
    $message = $_.Exception.GetBaseException().Message
    if ($Action -eq 'Replace') {
        Add-Type -AssemblyName PresentationFramework
        [void][Windows.MessageBox]::Show("Falha ao atualizar RenderLatch. $message", 'RenderLatch', 'OK', 'Error')
    } else {
        [Console]::Error.WriteLine($message)
    }
    exit 1
}
