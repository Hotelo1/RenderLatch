param([string]$OutputDirectory = (Join-Path $PSScriptRoot '..\dist'))

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$target = 'x86_64-pc-windows-msvc'
$original = @{
    RENDERLATCH_RELEASE = $env:RENDERLATCH_RELEASE
    RUSTFLAGS = $env:RUSTFLAGS
    CARGO_ENCODED_RUSTFLAGS = $env:CARGO_ENCODED_RUSTFLAGS
}
Push-Location $root
try {
    $env:RENDERLATCH_RELEASE = '1'
    Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    $env:RUSTFLAGS = '-C target-cpu=x86-64 -C target-feature=+crt-static'
    cargo build --release --locked --target $target --target-dir (Join-Path $root 'target\portable')
    if ($LASTEXITCODE -ne 0) { throw "Release build failed ($LASTEXITCODE)." }

    $output = (New-Item -ItemType Directory -Path $OutputDirectory -Force).FullName
    $filename = 'RenderLatch-windows-x64.exe'
    $binary = Join-Path $output $filename
    Copy-Item -LiteralPath (Join-Path $root "target\portable\$target\release\forge.exe") -Destination $binary -Force
    $version = (& $binary --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^RenderLatch \d+\.\d+\.\d+([+-]\S+)?$') {
        throw "The release executable did not return its version: $version"
    }
    $hash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $output "$filename.sha256"), "$hash  $filename`n")
    Write-Host "$version built at $binary"
} finally {
    foreach ($key in $original.Keys) {
        [Environment]::SetEnvironmentVariable($key, $original[$key], 'Process')
    }
    Pop-Location
}
