<#
  DLSS5 FORGE - build nativo
  Compila o programa para EXATAMENTE a CPU desta maquina.

  Uso:
    .\build.ps1            build nativo normal (rapido)
    .\build.ps1 -Pgo       build nativo + PGO (mais lento de compilar, mais rapido de rodar)
#>
param(
    [switch]$Pgo
)

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

function Step($n, $t) { Write-Host ""; Write-Host "  [$n] $t" -ForegroundColor Cyan }
function Ok($t)       { Write-Host "      OK  $t" -ForegroundColor Green }
function Warn($t)     { Write-Host "      !   $t" -ForegroundColor Yellow }
function Fail($t)     { Write-Host "      X   $t" -ForegroundColor Red }

Write-Host ""
Write-Host "  DLSS5 FORGE - build nativo" -ForegroundColor White
Write-Host "  ============================================================" -ForegroundColor DarkGray

Step 1 "Verificando toolchain Rust"
$rustc = Get-Command rustc -ErrorAction SilentlyContinue
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $rustc -or -not $cargo) {
    Fail "Rust nao encontrado."
    Write-Host "      Instale: https://rustup.rs  (winget install Rustlang.Rustup)" -ForegroundColor Yellow
    exit 1
}
Ok ((rustc -V) + " / " + (cargo -V))

Step 2 "Detectando esta CPU"
$cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
$cores    = (Get-CimInstance Win32_Processor | Select-Object -First 1).NumberOfCores
Ok "$cpuName ($cores nucleos)"

Step 3 "Escolhendo flags de compilacao"
# target-cpu=native: o compilador olha a CPU REAL e gera codigo para ela.
# Nada de AVX2 em Xeon antigo (que quebraria com 'illegal instruction').
$env:RUSTFLAGS = "-C target-cpu=native"
Ok "RUSTFLAGS = $env:RUSTFLAGS"
Ok "LTO fat + codegen-units=1 + panic=abort (vem do Cargo.toml)"

if ($Pgo) {
    Step 4 "PGO: preparando perfil de execucao"
    # rustup escreve progresso no stderr; com ErrorActionPreference=Stop isso
    # viraria excecao. Rodamos "solto" e conferimos o exit code.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    rustup component add llvm-tools-preview *> $null
    $ErrorActionPreference = $prevEap
    if ($LASTEXITCODE -eq 0) { Ok "llvm-tools pronto" } else { Warn "rustup component add falhou" }
    $sysroot = (rustc --print sysroot).Trim()
    $profdata = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-profdata.exe"
    if (-not (Test-Path $profdata)) {
        $profdata = Join-Path $sysroot "lib\rust\llvm-tools-preview\bin\llvm-profdata.exe"
    }
    if (-not (Test-Path $profdata)) {
        Warn "llvm-profdata nao encontrado - seguindo sem PGO"
        $Pgo = $false
    } else {
        Ok "llvm-profdata: $profdata"
        $pgoDir = Join-Path $PSScriptRoot "target\pgo"
        if (Test-Path $pgoDir) { Remove-Item $pgoDir -Recurse -Force }
        New-Item -ItemType Directory -Path $pgoDir -Force | Out-Null

        # Caminho relativo: RUSTFLAGS quebra em espacos, e esta pasta tem espacos.
        $env:RUSTFLAGS = "-C target-cpu=native -C profile-generate=target/pgo"
        Write-Host "      compilando instrumentado..." -ForegroundColor DarkGray
        cargo build --release --quiet
        if ($LASTEXITCODE -ne 0) { Fail "build instrumentado falhou"; exit 1 }

        Write-Host "      executando cargas reais para colher o perfil..." -ForegroundColor DarkGray
        & ".\target\release\forge.exe" detect  | Out-Null
        & ".\target\release\forge.exe" plan --json | Out-Null
        & ".\target\release\forge.exe" doctor | Out-Null

        $raw = Get-ChildItem -Path $pgoDir -Filter *.profraw -Recurse -ErrorAction SilentlyContinue
        if (-not $raw) { Warn "nenhum .profraw gerado - seguindo sem PGO"; $Pgo = $false }
        else {
            $merged = Join-Path $pgoDir "merged.profdata"
            & $profdata merge -o $merged $pgoDir | Out-Null
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path $merged)) {
                Warn "merge do perfil falhou - seguindo sem PGO"; $Pgo = $false
            } else {
                Ok "perfil gerado: $merged"
                $env:RUSTFLAGS = "-C target-cpu=native -C profile-use=target/pgo/merged.profdata"
            }
        }
    }
}

Step 5 "Compilando (release)"
$sw = [System.Diagnostics.Stopwatch]::StartNew()
cargo build --release --quiet
if ($LASTEXITCODE -ne 0) { Fail "compilacao falhou"; exit 1 }
$sw.Stop()
Ok ("compilado em " + [math]::Round($sw.Elapsed.TotalSeconds,1) + "s")

$exe = Join-Path $PSScriptRoot "target\release\forge.exe"
if (-not (Test-Path $exe)) { Fail "forge.exe nao encontrado"; exit 1 }
$kb = [math]::Round((Get-Item $exe).Length / 1KB)

Copy-Item $exe (Join-Path $PSScriptRoot "forge.exe") -Force
Ok "forge.exe  ($kb KB)"

Step 6 "Autoteste do binario"
& (Join-Path $PSScriptRoot "forge.exe") selftest

Write-Host "  ============================================================" -ForegroundColor DarkGray
Write-Host "  Pronto. Proximo passo:" -ForegroundColor White
Write-Host "    .\forge.exe doctor" -ForegroundColor Gray
Write-Host "    .\forge.exe plan" -ForegroundColor Gray
Write-Host "    .\forge.exe apply --game `"C:\...\jogo.exe`"" -ForegroundColor Gray
Write-Host ""
