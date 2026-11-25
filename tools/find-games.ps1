param(
    [string[]]$SteamRoot,
    [string]$EpicManifestRoot = (Join-Path $env:ProgramData 'Epic\EpicGamesLauncher\Data\Manifests')
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

function Read-VdfPairs([string]$File) {
    try {
        $text = Get-Content -LiteralPath $File -Raw -Encoding UTF8
    } catch {
        return
    }
    if ([string]::IsNullOrWhiteSpace($text)) { return }
    foreach ($pair in [regex]::Matches($text, '(?m)^\s*"(?<key>[^"]+)"[ \t]+"(?<value>(?:\\.|[^"\\])*)"')) {
        @{
            Key = $pair.Groups['key'].Value
            Value = $pair.Groups['value'].Value -replace '\\(["\\])', '$1'
        }
    }
}

function Read-RegistryValue([string]$Key, [string]$Name) {
    $entry = Get-ItemProperty -LiteralPath $Key -Name $Name -ErrorAction SilentlyContinue
    if ($entry) { $entry.$Name }
}

function New-Game([string]$Name, [string]$Store, [string]$Folder, [string]$Launch = '', [string]$Cover = '') {
    if ([string]::IsNullOrWhiteSpace($Name) -or -not [IO.Directory]::Exists($Folder)) { return }
    try {
        $folderPath = [IO.Path]::GetFullPath($Folder)
        if ($folderPath -ne [IO.Path]::GetPathRoot($folderPath)) { $folderPath = $folderPath.TrimEnd('\') }
    } catch {
        return
    }
    $selected = $folderPath
    if (-not [string]::IsNullOrWhiteSpace($Launch)) {
        try {
            $exe = [IO.Path]::GetFullPath([IO.Path]::Combine($folderPath, $Launch.Replace('/', '\')))
            $prefix = $folderPath.TrimEnd('\') + '\'
            if ($exe.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and
                    [IO.Path]::GetExtension($exe) -eq '.exe' -and [IO.File]::Exists($exe)) {
                $selected = $exe
            }
        } catch {
            # Keep the folder as the selectable target when a launcher hint is invalid.
        }
    }
    [pscustomobject]@{ Name = $Name.Trim(); Store = $Store; Folder = $folderPath; Target = $selected; Cover = $Cover }
}

function Find-SteamGames {
    $roots = @($SteamRoot | Where-Object { [IO.Directory]::Exists($_) } | Select-Object -Unique)
    foreach ($base in $roots) {
        $libraries = @($base)
        $libraries += @(Read-VdfPairs (Join-Path $base 'steamapps\libraryfolders.vdf') |
            Where-Object { ($_.Key -eq 'path' -or $_.Key -match '^\d+$') -and [IO.Directory]::Exists($_.Value) } |
            ForEach-Object Value)
        foreach ($library in ($libraries | Select-Object -Unique)) {
            $apps = Join-Path $library 'steamapps'
            foreach ($manifest in (Get-ChildItem -LiteralPath $apps -Filter 'appmanifest_*.acf' -File -ErrorAction SilentlyContinue)) {
                if ($manifest.Name -eq 'appmanifest_228980.acf') { continue } # Steamworks redistributables
                $fields = @{}
                foreach ($pair in (Read-VdfPairs $manifest.FullName)) {
                    if (-not $fields.ContainsKey($pair.Key)) { $fields[$pair.Key] = $pair.Value }
                }
                if (-not $fields.installdir -or $fields.installdir -in @('.', '..') -or
                        $fields.installdir -match '[\\/:]' -or $fields.name -match '^Proton(?:\b|$)|^Steam Linux Runtime') {
                    continue
                }
                $folder = Join-Path $apps ('common\' + $fields.installdir)
                $appId = $manifest.BaseName.Substring('appmanifest_'.Length)
                $cover = ''
                if ($appId -match '^\d+$') {
                    $cache = Join-Path $base 'appcache\librarycache'
                    foreach ($candidate in @((Join-Path $cache "$appId\library_600x900.jpg"), (Join-Path $cache "${appId}_library_600x900.jpg"))) {
                        if ([IO.File]::Exists($candidate)) { $cover = $candidate; break }
                    }
                    if (!$cover) {
                        $portrait = Get-ChildItem -LiteralPath (Join-Path $cache $appId) -Directory -ErrorAction SilentlyContinue |
                            ForEach-Object { Get-Item -LiteralPath (Join-Path $_.FullName 'library_600x900.jpg') -ErrorAction SilentlyContinue } |
                            Sort-Object LastWriteTime -Descending | Select-Object -First 1
                        if ($portrait) { $cover = $portrait.FullName }
                    }
                    if (!$cover) { $cover = "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/$appId/library_600x900.jpg" }
                }
                New-Game $fields.name 'Steam' $folder -Cover $cover
            }
        }
    }
}

function Find-EpicGames {
    foreach ($manifest in (Get-ChildItem -LiteralPath $EpicManifestRoot -Filter '*.item' -File -ErrorAction SilentlyContinue)) {
        try {
            $data = Get-Content -LiteralPath $manifest.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
        } catch {
            continue
        }
        if ($data.bIsApplication -eq $true -and $data.bIsIncompleteInstall -ne $true -and
                $data.CatalogNamespace -ne 'ue' -and $data.AppName -notmatch '^UE_') {
            New-Game $data.DisplayName 'Epic Games' $data.InstallLocation $data.LaunchExecutable
        }
    }
}

function Find-GogGames {
    foreach ($registryRoot in @('HKLM:\SOFTWARE\WOW6432Node\GOG.com\Games', 'HKLM:\SOFTWARE\GOG.com\Games')) {
        foreach ($key in (Get-ChildItem -LiteralPath $registryRoot -ErrorAction SilentlyContinue)) {
            $data = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
            if ($data) { New-Game $data.gameName 'GOG' $data.path $data.exe }
        }
    }
}

if (-not $PSBoundParameters.ContainsKey('SteamRoot')) {
    $SteamRoot = @(
        Read-RegistryValue 'HKCU:\Software\Valve\Steam' 'SteamPath'
        Read-RegistryValue 'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam' 'InstallPath'
        Read-RegistryValue 'HKLM:\SOFTWARE\Valve\Steam' 'InstallPath'
        if (${env:ProgramFiles(x86)}) { Join-Path ${env:ProgramFiles(x86)} 'Steam' }
    )
}

$byFolder = @{}
$games = @(Find-SteamGames; Find-EpicGames; Find-GogGames)
foreach ($game in $games) {
    if (-not $byFolder.ContainsKey($game.Folder) -or
            ($byFolder[$game.Folder].Target -eq $game.Folder -and $game.Target -ne $game.Folder)) {
        $byFolder[$game.Folder] = $game
    }
}
ConvertTo-Json -InputObject @($byFolder.Values | Sort-Object Name, Store, Folder) -Compress
