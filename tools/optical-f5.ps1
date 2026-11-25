param(
    [Parameter(Mandatory=$true)][string]$GameExe,
    [ValidateSet('Preview','Apply','Undo')][string]$Action = 'Preview',
    [ValidateSet('auto','dlss','fsr','xess','tensor')][string]$Generator = 'auto',
    [ValidateSet('winmm.dll','version.dll','dxgi.dll')][string]$PreferredProxy = 'dxgi.dll',
    [string]$Model
)
$ErrorActionPreference = 'Stop'
$game = Split-Path -Parent $GameExe
$store = Join-Path $game '_dlss5forge\optical-f5'
$journal = Join-Path $store 'files.txt'
$prepared = Join-Path $store 'staged'
$original = Join-Path $store 'original'
$ready = Join-Path $store 'installed'
$cache = Join-Path $env:LOCALAPPDATA 'RenderLatch\runtime\f5'
$zip = Join-Path $cache 'v0.1.39-Optiscaler-F5.zip'
$hash = '532ADA6B80D1CAAAE222734E053F6FC992EAD5BBCD01ECAAC851F26E6FD66ECF'
$allowed = @('OptiScaler','OptiScaler.ini','nvngx.dll_dlssnr.dll','nvngx_dlssnr.dll','winmm.dll','dxgi.dll','version.dll','dbghelp.dll','d3d12.dll')

function Set-Value($text,$section,$key,$value) {
    # Comments in the distribution INI may themselves contain section names.
    $pattern = "(?ims)(^\[$section\][ \t]*\r?\n(?:(?!^\[).)*?^$key[ \t]*=[ \t]*)[^\r\n]*"
    if (-not [regex]::IsMatch($text,$pattern)) { throw "Chave ausente: [$section] $key" }
    [regex]::Replace($text,$pattern,('${1}' + $value))
}

function Restore-Files {
    # Each rename is on the game volume. Staged entries identify moves still pending.
    $saved = Join-Path $store 'removed'
    [void][IO.Directory]::CreateDirectory($saved)
    foreach ($name in [IO.File]::ReadAllLines($journal)) {
        if ($name -notin $allowed) { throw 'Registro de arquivos F5 invalido.' }
        $target = Join-Path $game $name
        if (!(Test-Path -LiteralPath (Join-Path $prepared $name)) -and
            !(Test-Path -LiteralPath (Join-Path $saved $name)) -and (Test-Path -LiteralPath $target)) {
            Move-Item -LiteralPath $target -Destination $saved
        }
        if (Test-Path -LiteralPath (Join-Path $original $name)) {
            Move-Item -LiteralPath (Join-Path $original $name) -Destination $target
        }
    }
    $archive = Join-Path (Split-Path $store) ('f5-restored-' + [guid]::NewGuid())
    Move-Item -LiteralPath $store -Destination $archive
    Write-Output "Perfil anterior restaurado. Arquivos retirados guardados em $archive"
}

$lock = New-Object Threading.Mutex($false,'Local\RenderLatchOpticalF5Setup')
try {
    try { $held = $lock.WaitOne(0) } catch [Threading.AbandonedMutexException] { $held = $true }
    if (!$held) { throw 'Outra instalacao do Optical F5Low esta em andamento.' }
    if ($Action -eq 'Undo') {
        if (!(Test-Path -LiteralPath $journal)) { throw 'Nenhuma instalacao Optical F5Low registrada.' }
        Restore-Files
        return
    }
    if ((Test-Path -LiteralPath $journal) -and !(Test-Path -LiteralPath $ready)) {
        throw 'Instalacao incompleta. Use Desfazer instalacao para restaurar o perfil anterior.'
    }
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($GameExe))
    try {
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Executavel invalido.' }
        $reader.BaseStream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw 'O pacote Optical F5Low exige um jogo Windows x64.'
        }
    } finally { $reader.Dispose() }

    $proxy = $PreferredProxy
    # Replace an existing ReShade entry point for this route so the Feeder stays unloaded.
    if ((Test-Path -LiteralPath "$game\dxgi.dll") -and (Get-Item -LiteralPath "$game\dxgi.dll").VersionInfo.ProductName -match 'ReShade') { $proxy = 'dxgi.dll' }
    foreach ($candidate in 'winmm.dll','version.dll','dxgi.dll','dbghelp.dll','d3d12.dll') {
        $file = Join-Path $game $candidate
        if ((Test-Path -LiteralPath $file) -and ((Get-Item -LiteralPath $file).VersionInfo.ProductName -match 'OptiScaler')) {
            $proxy = $candidate
            break
        }
    }
    if ((Test-Path -LiteralPath "$game\dxgi.dll") -and $proxy -ne 'dxgi.dll' -and (Get-Item -LiteralPath "$game\dxgi.dll").VersionInfo.ProductName -match 'ReShade|OptiScaler') { throw 'Dois carregadores ativos detectados. Restaure a instalacao anterior antes de usar Optical F5Low.' }
    if ($proxy -eq 'dxgi.dll' -and (Test-Path -LiteralPath "$game\dxgi.dll")) {
        $product = (Get-Item -LiteralPath "$game\dxgi.dll").VersionInfo.ProductName
        if ($product -notmatch 'OptiScaler|ReShade') { throw 'dxgi.dll pertence a outro componente. Resolva o conflito de carregadores antes de instalar.' }
    }
    $output = switch ($Generator) {
        'fsr' { 'fsrfg' }
        'xess' { 'xefg' }
        'auto' {
            if ((Get-CimInstance Win32_VideoController).Name -match 'RTX (40|50)') { 'f5low' } else { 'fsrfg' }
        }
        default { 'f5low' }
    }
    $engine = if ($Generator -eq 'tensor') { 'xefg_tensor' } else { 'dlssg' }
    Write-Output "Optical F5Low 0.1.39 | $GameExe | $proxy | FGOutput=$output"
    if ($output -eq 'f5low') { Write-Output "Gerador F5Low: $engine" }
    Write-Output 'Entrada optica do OptiScaler. Ative Frame Generation pelo menu Insert dentro do jogo.'
    if ($Action -eq 'Preview') { Write-Output "Pacote de 201 MB em cache local, com backup em $store"; return }
    $iniPath = Join-Path $game 'OptiScaler.ini'
    if (Test-Path -LiteralPath $ready) {
        $ini = [IO.File]::ReadAllText($iniPath)
        $ini = Set-Value $ini FrameGen FGOutput $output
        $ini = Set-Value $ini F5Low FrameGenerator $engine
        $pending = Join-Path $store 'profile.tmp'
        [IO.File]::WriteAllText($pending,$ini)
        [IO.File]::Replace($pending,$iniPath,(Join-Path $store 'previous-profile.ini'))
        return
    }
    [void][IO.Directory]::CreateDirectory($cache)
    if (!(Test-Path -LiteralPath $zip) -or (Get-FileHash -LiteralPath $zip).Hash -ne $hash) {
        $part = "$zip.partial"
        Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/janblade/OptiScaler-F5-DLSSNR-Multipass/releases/download/v0.1.39/v0.1.39-Optiscaler-F5.zip" -OutFile $part
        if ((Get-FileHash -LiteralPath $part).Hash -ne $hash) { throw 'Falha de integridade no download do Optical F5Low.' }
        Move-Item -LiteralPath $part -Destination $zip -Force
    }
    $work = Join-Path $env:TEMP ('renderlatch-f5-' + [guid]::NewGuid())
    try {
        Expand-Archive -LiteralPath $zip -DestinationPath $work
        $ini = [IO.File]::ReadAllText("$work\OptiScaler.ini")
        foreach ($setting in @(
            @('FrameGen','Enabled','false'),@('FrameGen','FGInput','upscaler'),@('FrameGen','FGOutput',$output),
            @('F5Low','FrameGenerator',$engine),@('DlssNr','Enabled','false'),@('DlssNr','NativeMotion','true'),
            @('DlssNr','NativeUpscaler','true'),@('DlssNr','NativeDepthFinder','true'),@('Plugins','LoadReshade','false')
        )) { $ini = Set-Value $ini $setting[0] $setting[1] $setting[2] }
        [IO.File]::WriteAllText("$work\OptiScaler.ini",$ini)
        if (Test-Path -LiteralPath $store) {
            # No journal means staging was interrupted before any game file moved.
            Move-Item -LiteralPath $store -Destination (Join-Path (Split-Path $store) ('f5-incomplete-' + [guid]::NewGuid()))
        }
        [void][IO.Directory]::CreateDirectory($prepared)
        [void][IO.Directory]::CreateDirectory($original)
        foreach ($name in 'OptiScaler.ini','OptiScaler','nvngx.dll_dlssnr.dll') {
            Copy-Item -LiteralPath (Join-Path $work $name) -Destination $prepared -Recurse -Force
        }
        Copy-Item -LiteralPath "$work\OptiScaler.dll" -Destination (Join-Path $prepared $proxy)
        if ($Model -and !(Test-Path -LiteralPath "$game\nvngx_dlssnr.dll")) { Copy-Item -LiteralPath $Model -Destination $prepared }
        $names = @(Get-ChildItem -LiteralPath $prepared -Name)
        [IO.File]::WriteAllLines($journal,[string[]]$names)
        try {
            foreach ($name in $names) {
                $target = Join-Path $game $name
                if (Test-Path -LiteralPath $target) { Move-Item -LiteralPath $target -Destination $original }
                Move-Item -LiteralPath (Join-Path $prepared $name) -Destination $target
            }
            [IO.File]::WriteAllText($ready,'0.1.39')
        } catch { Restore-Files; throw }
    } finally {
        if ((Test-Path -LiteralPath $work) -and (Split-Path ([IO.Path]::GetFullPath($work))) -eq ([IO.Path]::GetFullPath($env:TEMP)).TrimEnd('\')) {
            Remove-Item -LiteralPath $work -Recurse -Force
        }
    }
} finally { if ($held) { $lock.ReleaseMutex() }; $lock.Dispose() }
