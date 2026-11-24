<#
  Menu interativo do DLSS5 Forge.
  Chamado por INICIAR.cmd (duplo clique) ou por: forge.exe menu
#>

$ErrorActionPreference = 'Continue'
$root  = Split-Path $PSScriptRoot -Parent
$forge = Join-Path $root 'forge.exe'

Add-Type -AssemblyName System.Windows.Forms | Out-Null

function Show-Header {
    Clear-Host
    Write-Host ""
    Write-Host "   RENDERLATCH" -ForegroundColor White
    Write-Host "   ============================================================" -ForegroundColor DarkGray
    if (Test-Path $forge) {
        $kb = [math]::Round((Get-Item $forge).Length / 1KB)
        Write-Host "   compilado: $kb KB   |   $root" -ForegroundColor DarkGray
    } else {
        Write-Host "   forge.exe ainda nao existe - use a opcao 7" -ForegroundColor Yellow
    }

    $kit = Join-Path $root 'runtime\dlss5\nvngx_dlssnr.dll'
    if (Test-Path $kit) {
        $mb = [math]::Round((Get-Item $kit).Length / 1MB, 0)
        Write-Host "   kit DLSS 5: pronto ($mb MB de modelo)" -ForegroundColor Green
    } else {
        Write-Host "   kit DLSS 5: ausente - use a opcao 8" -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "   1  Diagnostico do PC        (o que esta errado, com correcao)"
    Write-Host "   2  Retrato do hardware      (CPU, GPU, PCIe, ReBAR, RAM)"
    Write-Host "   3  Plano DLSS 5             (veredito + ajustes da maquina)"
    Write-Host "   4  Configurar um jogo       (perfil + launcher)"
    Write-Host "   5  Instalar runtime NVIDIA  (DLLs oficiais no projeto)"
    Write-Host "   6  Importar DLL da DLSS 5   (de um jogo que voce tem)"
    Write-Host "   7  Recompilar para este PC"
    Write-Host "   8  Baixar o kit da DLSS 5" -ForegroundColor Cyan
    Write-Host "   9  APLICAR a DLSS 5 num jogo" -ForegroundColor Cyan
    Write-Host "   0  Sair"
    Write-Host ""
}

function Wait-Back {
    Write-Host ""
    Read-Host "   Enter para voltar ao menu" | Out-Null
}

function Invoke-Forge([string[]]$argv) {
    if (-not (Test-Path $forge)) {
        Write-Host "   forge.exe nao encontrado. Rode a opcao 7." -ForegroundColor Red
        return
    }
    & $forge @argv
}

function Pick-Exe {
    $dlg = New-Object System.Windows.Forms.OpenFileDialog
    $dlg.Title  = "Escolha o executavel do jogo"
    $dlg.Filter = "Executaveis (*.exe)|*.exe"
    if ($dlg.ShowDialog() -eq 'OK') { return $dlg.FileName }
    return $null
}

function Pick-Folder([string]$title = "Escolha a pasta do jogo (a que tem as DLLs)") {
    $dlg = New-Object System.Windows.Forms.FolderBrowserDialog
    $dlg.Description = $title
    if ($dlg.ShowDialog() -eq 'OK') { return $dlg.SelectedPath }
    return $null
}

function Run-Build {
    $ps = Join-Path $root 'build.ps1'
    if (-not (Test-Path $ps)) {
        Write-Host "   build.ps1 nao encontrado." -ForegroundColor Red
        return
    }
    Write-Host ""
    Write-Host "   Recompilando para esta CPU..." -ForegroundColor Cyan
    & powershell -NoProfile -ExecutionPolicy Bypass -File $ps
}

function Get-Kit {
    $ps = Join-Path $root 'tools\fetch-dlss5.ps1'
    if (-not (Test-Path $ps)) {
        Write-Host "   tools\fetch-dlss5.ps1 nao encontrado." -ForegroundColor Red
        return
    }
    & powershell -NoProfile -ExecutionPolicy Bypass -File $ps
}

# Mostra o plano e pede confirmacao antes de tocar em qualquer coisa.
function Invoke-Dlss5([string]$dir) {
    Invoke-Forge @('dlss5', '--game', $dir)
    if (-not (Test-Path -LiteralPath $forge) -or $LASTEXITCODE -ne 0) { return }
    Write-Host ""
    $c = Read-Host "   Aplicar agora? (s/N)"
    if ($c -match '^(s|sim|y|yes)$') {
        Write-Host ""
        Invoke-Forge @('dlss5', '--game', $dir, '--yes')
    } else {
        Write-Host "   nada foi alterado." -ForegroundColor DarkGray
    }
}

while ($true) {
    Show-Header
    $opt = Read-Host "   Escolha"

    switch ($opt.Trim()) {
        '1' { Clear-Host; Invoke-Forge @('doctor'); Wait-Back }
        '2' { Clear-Host; Invoke-Forge @('detect'); Wait-Back }
        '3' { Clear-Host; Invoke-Forge @('plan');   Wait-Back }
        '4' {
            $exe = Pick-Exe
            if ($exe) { Clear-Host; Invoke-Forge @('apply','--game',$exe) } else { Write-Host "   cancelado." -ForegroundColor DarkGray }
            Wait-Back
        }
        '5' {
            Clear-Host
            $script = Join-Path $root 'tools\fetch-runtime.ps1'
            & powershell -NoProfile -ExecutionPolicy Bypass -File $script
            Wait-Back
        }
        '6' {
            $dir = Pick-Folder "Pasta do jogo que tem a DLSS 5 (ex.: NBA 2K27)"
            if ($dir) {
                Clear-Host
                $script = Join-Path $root 'tools\fetch-runtime.ps1'
                & powershell -NoProfile -ExecutionPolicy Bypass -File $script -FromGame $dir
            } else {
                Write-Host "   cancelado." -ForegroundColor DarkGray
            }
            Wait-Back
        }
        '7' { Run-Build; Wait-Back }
        '8' {
            Clear-Host
            Get-Kit
            Wait-Back
        }
        '9' {
            $dir = Pick-Folder "Pasta do JOGO (a que tem o .exe; ex.: ...\Binaries\Win64)"
            if (-not $dir) { Write-Host "   cancelado." -ForegroundColor DarkGray; Wait-Back; continue }

            $inst = Join-Path $dir '_dlss5forge\install.txt'
            $feeder = Join-Path $dir 'dlss5-feed.dlss5oneclick'
            if ((Test-Path $inst) -or (Test-Path $feeder)) {
                Clear-Host
                Write-Host ""
                Write-Host "   Esta pasta ja tem DLSS 5 aplicada pelo forge." -ForegroundColor Yellow
                Write-Host ""
                Write-Host "   1  Desfazer (restaura os arquivos originais)"
                Write-Host "   2  Reaplicar por cima (atualiza o kit/escala)"
                Write-Host "   0  Cancelar"
                Write-Host ""
                $o2 = Read-Host "   Escolha"
                Clear-Host
                switch ($o2.Trim()) {
                    '1' { Invoke-Forge @('dlss5','--game',$dir,'--undo') }
                    '2' { Invoke-Dlss5 $dir }
                    default { Write-Host "   cancelado." -ForegroundColor DarkGray }
                }
            } else {
                Clear-Host
                Invoke-Dlss5 $dir
            }
            Wait-Back
        }
        '0' { return }
        default {
            if ($opt.Trim() -ne '') { Write-Host "   opcao invalida." -ForegroundColor Yellow }
            Start-Sleep -Milliseconds 400
        }
    }
}
