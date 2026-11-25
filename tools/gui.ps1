param(
    [string]$Path = '',
    [switch]$NoTitleBar,
    [string]$ExecutablePath = '',
    [string]$RuntimeRoot = '',
    [switch]$Standalone
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, System.Windows.Forms

$root = Split-Path $PSScriptRoot -Parent
$forge = if ($ExecutablePath) { $ExecutablePath } else { Join-Path $root 'forge.exe' }
$buildScript = Join-Path $root 'build.ps1'
$kitScript = Join-Path $PSScriptRoot 'fetch-dlss5.ps1'
$menuScript = Join-Path $PSScriptRoot 'menu.ps1'
$gameScan = Join-Path $PSScriptRoot 'find-games.ps1'
$updateScript = Join-Path $PSScriptRoot 'update-app.ps1'
$runtimeFolder = if ($RuntimeRoot) { $RuntimeRoot } else { Join-Path $root 'runtime' }
$kitFolder = Join-Path $runtimeFolder 'dlss5'
$managedOneClick = Join-Path $runtimeFolder 'tools\dlss5oneclick.exe'
$defaultOneClick = Join-Path $PSScriptRoot 'dlss5oneclick.exe'
$oneClickPreference = Join-Path $env:LOCALAPPDATA 'RenderLatch\oneclick-path.txt'
$psExe = Join-Path ([Environment]::SystemDirectory) 'WindowsPowerShell\v1.0\powershell.exe'
$state = @{ Job = $null; PreviewedPath = $null; OneClick = $null; SelectedGame = $null; Route = ''; Games = @(); LibraryError = $false }
$appVersion = if ($Standalone -and [IO.File]::Exists($forge)) {
    [Diagnostics.FileVersionInfo]::GetVersionInfo($forge).ProductVersion
}
$updateState = @{ Check = $null; Tag = ''; NextCheck = [datetime]::MinValue; ManualCheck = $false }

try {
    $xaml = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'gui.xaml'))
    $window = [Windows.Markup.XamlReader]::Parse($xaml)
} catch {
    [void][Windows.MessageBox]::Show("Falha ao abrir a interface.$([Environment]::NewLine)$($_.Exception.Message)", 'RenderLatch', 'OK', 'Error')
    exit 1
}

$ui = @{}
foreach ($name in @('NavGames', 'NavPc', 'NavTools', 'WorkScroll', 'GamePage', 'PcPage', 'ToolsPage',
        'GameSelection', 'SelectionHint', 'RouteSummary', 'OptiOptions', 'OptiOptionsHint', 'OneClickStatus', 'KitStatus',
        'LibraryView', 'LibrarySearch', 'LibraryStatus', 'LibraryTiles', 'ManualGame', 'GameDetails', 'BackToLibrary', 'SelectedGameCover',
        'AppLogo', 'ManualTarget', 'GamePath', 'Browse', 'BrowseExe', 'FindGames', 'ChooseOneClick', 'OneClickReleases', 'UseFeeder',
        'InstallBar', 'InstallTitle', 'GameHint', 'Preview', 'Apply', 'Inventory', 'Undo', 'Optimize', 'RestoreResolution',
        'OpticalF5', 'OpticalOptions', 'OpticalGenerator', 'Upscaler', 'FrameGen', 'FrameGenSource', 'FrameGenInput', 'UseInGameMenu', 'Detect', 'Plan', 'Doctor',
        'Download', 'ImportModel', 'DeveloperTools', 'Build', 'Menu', 'ActivityPane', 'Activity', 'OperationCard', 'OperationTitle', 'OperationResult',
        'ExpandLog', 'CopyLog', 'ClearLog', 'Status', 'Progress', 'AppUpdate', 'AppVersion', 'AppUpdateButton', 'AppUpdateLabel', 'AppUpdateStatus')) {
    $ui[$name] = $window.FindName($name)
}

$icon = if ($Standalone) { Join-Path $PSScriptRoot 'renderlatch.ico' } else { Join-Path $root 'assets\renderlatch.ico' }
if ([IO.File]::Exists($icon)) {
    $decoder = [Windows.Media.Imaging.BitmapDecoder]::Create([Uri]::new($icon), 'PreservePixelFormat', 'OnLoad')
    $window.Icon = $decoder.Frames[$decoder.Frames.Count - 1]
    $ui.AppLogo.Source = $window.Icon
}
if ($Standalone) { $ui.DeveloperTools.Visibility = 'Collapsed' }
if ([IO.File]::Exists($forge)) {
    Add-Type -Path (Join-Path $PSScriptRoot 'TaskbarIdentity.cs')
    $window.Add_SourceInitialized({
        $handle = [Windows.Interop.WindowInteropHelper]::new($window).Handle
        [RenderLatch.TaskbarIdentity]::Apply($handle, $forge)
    })
}

function Get-OneClickPath {
    foreach ($candidate in @($state.OneClick, $managedOneClick, $defaultOneClick)) {
        if ($candidate -and [IO.File]::Exists($candidate)) { return $candidate }
    }
}

function Update-Controls {
    $busy = $null -ne $state.Job
    $ui.AppUpdateButton.IsEnabled = !$busy -and !$updateState.Check
    $ui.AppUpdateLabel.Text = if ($state.Job -and $state.Job.AppUpdate) { 'Baixando…' }
        elseif ($updateState.Tag) { 'Atualizar' } else { 'Atualizações' }
    $ui.AppVersion.Text = if ($updateState.Tag) { $updateState.Tag } elseif ($appVersion) { "v$appVersion" }
    $ui.AppUpdateButton.Background = if ($updateState.Tag) { $window.FindResource('Accent') } else { '#1A2226' }
    $ui.AppUpdateButton.Foreground = if ($updateState.Tag) { '#182611' } else { $window.FindResource('WindowText') }
    $updateAction = if ($updateState.Tag) { "Atualizar para $($updateState.Tag)" } else { 'Verificar atualizações do RenderLatch' }
    $ui.AppUpdateButton.ToolTip = $updateAction
    [Windows.Automation.AutomationProperties]::SetName($ui.AppUpdateButton, $updateAction)
    $hasForge = [IO.File]::Exists($forge)
    $hasPath = -not [string]::IsNullOrWhiteSpace($ui.GamePath.Text)
    $kitFiles = @('dlss5\OptiScaler.dll', 'dlss5\OptiScaler.ini', 'dlss5\nvngx.dll_dlssnr.dll', 'dlss5\nvngx_dlssnr.dll',
        'dlss5\feeder\OptiScaler.dll', 'dlss5\feeder\OptiScaler.ini', 'nvngx_dlss.dll', 'sl.interposer.dll', 'nvngx_dlssg.dll',
        'tools\dlss5oneclick.exe')
    $missingFiles = @($kitFiles | Where-Object { -not [IO.File]::Exists((Join-Path $runtimeFolder $_)) })
    $kitPresent = $missingFiles.Count -eq 0

    $ui.KitStatus.Text = if ($kitPresent) { 'Disponível' } else { 'Incompleto' }
    $ui.KitStatus.Foreground = $window.FindResource($(if ($kitPresent) { 'Accent' } else { 'Warning' }))
    $ui.KitStatus.ToolTip = if (-not $kitPresent) { $missingFiles -join [Environment]::NewLine }
    $ui.GamePath.IsEnabled = $ui.Browse.IsEnabled = $ui.BrowseExe.IsEnabled = -not $busy
    $ui.FindGames.IsEnabled = -not $busy -and [IO.File]::Exists($gameScan)
    $ui.ManualGame.IsEnabled = $ui.BackToLibrary.IsEnabled = $ui.LibraryTiles.IsEnabled = -not $busy
    foreach ($name in @('Detect', 'Plan', 'Doctor')) {
        $ui[$name].IsEnabled = $hasForge -and -not $busy
    }
    foreach ($name in @('Inventory', 'Undo', 'Optimize', 'RestoreResolution')) {
        $ui[$name].IsEnabled = $hasForge -and $hasPath -and -not $busy
    }
    $ui.Preview.IsEnabled = $hasForge -and $hasPath -and -not $busy -and $state.Route -ne 'pending'
    $ui.Apply.IsEnabled = $ui.Preview.IsEnabled -and $null -ne $state.PreviewedPath
    $ui.UseInGameMenu.Visibility = if ($state.Route -in @('feeder', 'feeder-dx11', 'feeder-ats-dx11')) { 'Visible' } else { 'Collapsed' }
    $ui.UseInGameMenu.IsEnabled = -not $busy -and $state.Route -in @('feeder', 'feeder-dx11', 'feeder-ats-dx11')
    $showOpti = $state.Route -in @('opti', 'feeder-overlay', 'feeder-overlay-dx11', 'feeder-ats-overlay') -or ($state.Route -in @('feeder', 'feeder-dx11', 'feeder-ats-dx11') -and $ui.UseInGameMenu.IsChecked)
    $ui.OptiOptions.Visibility = if ($showOpti) { 'Visible' } else { 'Collapsed' }
    $ui.OptiOptionsHint.Text = switch ($state.Route) {
        'feeder-new' { 'Primeiro, confira a prévia e aplique o Feeder. O menu do OptiScaler fica disponível em jogos compatíveis com DirectX 11 ou 12.' }
        'feeder-dx11' { 'DirectX 11 detectado. Marque o menu para configurar upscaling e Frame Generation via OptiFG.' }
        'feeder-ats-dx11' { 'Marque a opção para adicionar o OptiScaler. O perfil do ATS em DX11 inicia sem a ponte de Frame Generation.' }
        'feeder-ats-overlay' { 'Abra o OptiScaler com Insert. O perfil do ATS em DX11 inicia sem a ponte de Frame Generation.' }
        'feeder-unknown-api' { 'Feeder instalado. Abra e feche o jogo em DirectX 11 ou 12 e selecione o jogo novamente.' }
        'feeder' {
            if ($showOpti) { 'O Forge prepara o overlay no Feeder. Abra com Insert dentro do jogo.' }
            else { 'Marque a opção abaixo para adicionar o menu do OptiScaler ao Feeder.' }
        }
        'feeder-overlay-dx11' { 'Abra com Insert. O Frame Generation em DirectX 11 usa a entrada Upscaler (OptiFG).' }
        'feeder-overlay' { 'No jogo, abra com Insert. Ao trocar a saída do Frame Generation, use Save INI e reinicie.' }
        'opti-dx11' { 'O jogo está em DirectX 11. Estes perfis do OptiScaler são para DirectX 12.' }
        'opti' { 'Escolha aqui os valores iniciais ou abra o menu com Insert dentro do jogo.' }
        'pending' { 'Aguarde a verificação de compatibilidade.' }
        default { if (-not $hasPath) { 'Escolha um jogo para ver as configurações disponíveis.' } }
    }
    $ui.Upscaler.IsEnabled = -not $busy -and $showOpti
    $atsDx11 = $state.Route -in @('feeder-ats-dx11', 'feeder-ats-overlay')
    $ui.FrameGen.IsEnabled = -not $busy -and $showOpti -and -not $atsDx11
    $dx11Overlay = $state.Route -in @('feeder-dx11', 'feeder-overlay-dx11')
    foreach ($item in $ui.FrameGenInput.Items) {
        $item.IsEnabled = -not $dx11Overlay -or $item.Tag -in @('auto', 'upscaler')
    }
    if ($dx11Overlay -and -not $ui.FrameGenInput.SelectedItem.IsEnabled) { $ui.FrameGenInput.SelectedIndex = 0 }
    $needsSource = $showOpti -and -not $atsDx11 -and $ui.FrameGen.SelectedItem.Tag -notin @($null, '', 'off')
    $ui.FrameGenSource.Visibility = if ($needsSource) { 'Visible' } else { 'Collapsed' }
    $ui.FrameGenInput.IsEnabled = $needsSource -and -not $busy
    $ui.ChooseOneClick.IsEnabled = $ui.UseFeeder.IsEnabled = -not $busy
    $oneClick = Get-OneClickPath
    $ui.OneClickStatus.Text = if ($oneClick -eq $managedOneClick) { 'Instalado pelo kit para a rota Feeder.' }
        elseif ($oneClick) { 'Executável personalizado vinculado à rota Feeder.' }
        else { 'Incluído no kit completo.' }
    $ui.OneClickStatus.ToolTip = $oneClick
    $ui.RouteSummary.Text = switch ($state.Route) {
        'feeder-new' { 'Feeder · primeira instalação' }
        'feeder-ats-dx11' { 'Feeder · DirectX 11' }
        'feeder-ats-overlay' { 'Feeder + OptiScaler · DirectX 11' }
        'feeder-dx11' { 'Feeder · DirectX 11' }
        'feeder-unknown-api' { 'Feeder · API a confirmar' }
        'feeder-overlay-dx11' { 'Feeder + OptiScaler · DirectX 11' }
        'feeder-overlay' { 'Feeder + OptiScaler · DirectX 12' }
        'feeder' { if ($ui.UseFeeder.IsChecked) { 'Feeder · seleção manual' } else { 'Feeder' } }
        'opti' { 'OptiScaler' }
        'opti-dx11' { 'OptiScaler · DirectX 11' }
        'pending' { 'Identificando o jogo…' }
        default { if ($hasPath -and $hasForge) { 'Compatibilidade ainda não identificada' } }
    }
    $optical = $ui.OpticalF5.IsChecked -or $state.Route -eq 'optical-installed'
    $ui.OpticalF5.IsEnabled = -not $busy -and $state.Route -ne 'optical-installed'
    $ui.OpticalOptions.Visibility = if ($optical) { 'Visible' } else { 'Collapsed' }
    $ui.OpticalGenerator.IsEnabled = -not $busy
    if ($optical) {
        $ui.RouteSummary.Text = 'OptiScaler F5 · entrada óptica'
        $ui.OptiOptionsHint.Text = 'Para jogos Windows x64 em DX11, DX12 ou Vulkan. Ative Frame Generation com Insert dentro do jogo. A compatibilidade varia por jogo e GPU.'
        $ui.UseInGameMenu.Visibility = $ui.OptiOptions.Visibility = 'Collapsed'
        $ui.UseFeeder.IsEnabled = $false
        if ($state.Route -eq 'optical-installed') { $ui.Optimize.IsEnabled = $false }
    }
    $ui.RouteSummary.Visibility = if ($ui.RouteSummary.Text) { 'Visible' } else { 'Collapsed' }
    $ui.OptiOptionsHint.Visibility = if ($ui.OptiOptionsHint.Text) { 'Visible' } else { 'Collapsed' }
    $ui.GameSelection.Text = if ($state.SelectedGame) {
        $state.SelectedGame.Name
    } elseif ($hasPath) {
        ($ui.GamePath.Text.Trim().TrimEnd('\', '/') -split '[\\/]')[-1]
    } else { 'Nenhum jogo selecionado' }
    $ui.SelectionHint.Text = if ($state.SelectedGame) {
        $state.SelectedGame.Store
    } elseif ($hasPath) { 'Seleção manual' } else { 'Escolha a pasta ou o executável.' }
    $ui.SelectedGameCover.Source = if ($state.SelectedGame.Artwork) { $state.SelectedGame.Artwork } else { $state.SelectedGame.Icon }
    $ui.Build.IsEnabled = -not $busy -and [IO.File]::Exists($buildScript)
    $ui.Download.IsEnabled = -not $busy -and [IO.File]::Exists($kitScript)
    $ui.ImportModel.IsEnabled = $ui.Download.IsEnabled
    $ui.Menu.IsEnabled = -not $busy -and [IO.File]::Exists($menuScript)
    $ui.ClearLog.IsEnabled = -not $busy -and $ui.Activity.Text.Length -gt 0
    $ui.CopyLog.IsEnabled = $ui.Activity.Text.Length -gt 0
    $ui.Progress.IsIndeterminate = $busy
    $ui.Progress.Visibility = if ($busy) { 'Visible' } else { 'Hidden' }
    $hasPreview = $null -ne $state.PreviewedPath
    $ui.InstallTitle.Text = if ($hasPreview) { 'Prévia pronta' } else { 'Instalação' }
    $ui.InstallTitle.Foreground = $window.FindResource($(if ($hasPreview) { 'Accent' } else { 'WindowText' }))
    $ui.GameHint.Text = if (-not $hasForge) {
        'Use Recompilar para gerar o forge.exe.'
    } elseif (-not $hasPath) {
        'Selecione um jogo para habilitar a prévia.'
    } elseif ($hasPreview) {
        'Confira a atividade e feche o jogo antes de aplicar.'
    } elseif ($state.Route -eq 'pending') {
        'Verificando a rota de instalação…'
    } elseif ($optical) {
        'Confira a prévia do Optical F5Low para o executável selecionado.'
    } elseif (($ui.UseFeeder.IsChecked -or $state.Route -like 'feeder*') -and -not $oneClick) {
        'Em Ferramentas, baixe o kit completo para usar o Feeder.'
    } elseif ($state.Route -like 'feeder*') {
        'Rota Feeder. Confira os componentes em Ver prévia.'
    } else {
        'Revise o destino e os arquivos antes de aplicar.'
    }
}

function Set-OperationCard([string]$Title, [string]$Result, [string]$Tone = 'Muted') {
    $ui.OperationCard.Visibility = 'Visible'
    $ui.OperationTitle.Text = $Title
    $ui.OperationResult.Text = $Result
    $ui.OperationResult.Foreground = $window.FindResource($Tone)
}

function Add-Activity([string]$Text) {
    $follow = $ui.Activity.VerticalOffset -ge ($ui.Activity.ExtentHeight - $ui.Activity.ViewportHeight - 2)
    $ui.Activity.AppendText($Text + [Environment]::NewLine)
    if ($follow) { $ui.Activity.ScrollToEnd() }
    $ui.CopyLog.IsEnabled = $true
}

function Show-Error([string]$Message) {
    $ui.Status.Text = $Message
    Set-OperationCard 'Não foi possível continuar' $Message 'Failure'
    Add-Activity "  $Message"
    Update-Controls
}

# ProcessStartInfo on Windows PowerShell 5.1 takes a single Windows command line.
function Join-WindowsArguments([string[]]$ArgumentList) {
    ($ArgumentList | ForEach-Object {
        '"' + (($_ -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
    }) -join ' '
}

function Set-AppUpdateStatus([string]$Text, [string]$Tone = 'Muted', [string]$Details = '') {
    $ui.AppUpdateStatus.Text = $Text
    $ui.AppUpdateStatus.Foreground = $window.FindResource($Tone)
    $ui.AppUpdateStatus.Visibility = if ($Text) { 'Visible' } else { 'Collapsed' }
    $ui.AppUpdateStatus.ToolTip = if ($Details) { $Details }
}

function Start-UpdateCheck([switch]$Manual) {
    if ($updateState.Check -or $state.Job) { return }
    $updateState.NextCheck = [datetime]::UtcNow.AddMinutes(15)
    $updateState.ManualCheck = $Manual.IsPresent
    if ($Manual) { Set-AppUpdateStatus 'Verificando…' }
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $psExe
    $info.Arguments = Join-WindowsArguments @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $updateScript,
        '-Action', 'Check', '-Executable', $forge, '-CurrentVersion', $appVersion)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    try {
        $process = [Diagnostics.Process]::Start($info)
        $updateState.Check = @{
            Process = $process
            Output = $process.StandardOutput.ReadToEndAsync()
            ErrorOutput = $process.StandardError.ReadToEndAsync()
        }
    } catch {
        Set-AppUpdateStatus 'Falha na verificação' 'Warning' $_.Exception.GetBaseException().Message
    }
    Update-Controls
}

function Complete-UpdateDownload([string]$Output) {
    $pending = $Output | ConvertFrom-Json
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $psExe
    $info.Arguments = Join-WindowsArguments @('-NoProfile', '-NonInteractive', '-STA', '-ExecutionPolicy', 'Bypass', '-File', $updateScript,
        '-Action', 'Replace', '-Executable', $forge, '-CurrentVersion', $appVersion,
        '-ReleaseTag', $updateState.Tag, '-StagedFile', $pending.Path, '-ExpectedHash', $pending.Hash,
        '-GuiProcessId', [string]$PID)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    try {
        $helper = [Diagnostics.Process]::Start($info)
        $helper.Dispose()
    } catch {
        Remove-Item -LiteralPath $pending.Path -Force -ErrorAction SilentlyContinue
        throw
    }
    $window.Close()
}

function Start-Operation([string]$Label, [string]$FileName, [string[]]$ArgumentList,
        [string]$PreviewPath, [switch]$Diagnostic, [switch]$GameLibrary, [string]$RoutePath) {
    if ($null -ne $state.Job) { return }

    $logStart = $ui.Activity.Text.Length
    if (-not $RoutePath -and -not $GameLibrary) { Add-Activity ('[{0}] {1}' -f (Get-Date -Format 'HH:mm:ss'), $Label) }
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $FileName
    $info.Arguments = Join-WindowsArguments $ArgumentList
    $info.WorkingDirectory = $root
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($FileName -eq $forge -or $GameLibrary -or $ArgumentList -contains $kitScript -or $ArgumentList -contains $updateScript) {
        $info.StandardOutputEncoding = [Text.Encoding]::UTF8
        $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    }

    try {
        $process = [Diagnostics.Process]::Start($info)
    } catch {
        if ($RoutePath) { $state.Route = ''; Update-Controls }
        else { Show-Error "Falha ao iniciar $($Label.ToLowerInvariant()). $($_.Exception.GetBaseException().Message)" }
        return
    }

    $state.Job = @{
        Process = $process
        Label = $Label
        PreviewPath = $PreviewPath
        LogStart = $logStart
        Diagnostic = $Diagnostic.IsPresent
        Optimization = $ArgumentList -contains '--optimize'
        GameLibrary = $GameLibrary.IsPresent
        KitDownload = $ArgumentList -contains '-Complete' -and $ArgumentList -contains $kitScript
        AppUpdate = $ArgumentList -contains $updateScript
        RoutePath = $RoutePath
        RefreshRoute = ($FileName -eq $forge -and ($ArgumentList -contains '--yes' -or $ArgumentList -contains '--undo')) -or
            $ArgumentList -contains $buildScript
        Output = [Text.StringBuilder]::new()
        Streams = @(
            @{ Reader = $process.StandardOutput; Pending = $process.StandardOutput.ReadLineAsync() }
            @{ Reader = $process.StandardError; Pending = $process.StandardError.ReadLineAsync() }
        )
    }
    if (-not $RoutePath -and -not $GameLibrary) {
        $ui.Status.Text = "$Label em andamento…"
        Set-OperationCard $Label 'Em andamento…'
    }
    Update-Controls
    $timer.Start()
}

$timer = [Windows.Threading.DispatcherTimer]::new()
$timer.Interval = [TimeSpan]::FromMilliseconds(80)
$timer.Add_Tick({
    $job = $state.Job
    $batch = [Text.StringBuilder]::new()
    # Keep the dispatcher responsive even when a command emits many lines.
    foreach ($stream in $job.Streams) {
        for ($i = 0; $i -lt 100 -and $null -ne $stream.Pending -and $stream.Pending.IsCompleted; $i++) {
            if ($stream.Pending.IsFaulted -or $stream.Pending.IsCanceled) {
                [void]$batch.AppendLine('  Falha ao ler a saída do comando.')
                $stream.Pending = $null
                continue
            }
            $line = $stream.Pending.Result
            if ($null -eq $line) {
                $stream.Pending = $null
            } else {
                if (($job.GameLibrary -or $job.RoutePath -or $job.AppUpdate) -and $stream -eq $job.Streams[0]) {
                    [void]$job.Output.AppendLine($line)
                } else {
                    [void]$batch.AppendLine($line)
                    if ($job.KitDownload -and $line -match '\[([1-4])/4\] (.+)$') {
                        Set-OperationCard $job.Label "$($Matches[1]) de 4 · $($Matches[2])"
                    }
                }
                $stream.Pending = $stream.Reader.ReadLineAsync()
            }
        }
    }
    if ($batch.Length -gt 0 -and -not $job.RoutePath) {
        $lines = $batch.ToString()
        Add-Activity ($lines.Substring(0, $lines.Length - [Environment]::NewLine.Length))
    }

    if (-not $job.Process.HasExited -or $null -ne $job.Streams[0].Pending -or $null -ne $job.Streams[1].Pending) {
        return
    }

    $timer.Stop()
    $code = $job.Process.ExitCode
    $job.Process.Dispose()
    $state.Job = $null

    if ($job.AppUpdate) {
        if ($code -eq 0) {
            try {
                Complete-UpdateDownload $job.Output.ToString()
            } catch {
                Show-Error "Não foi possível iniciar a substituição do executável. $($_.Exception.GetBaseException().Message)"
            }
        } else {
            $updateState.Tag = ''
            Set-AppUpdateStatus 'Falha no download' 'Warning'
            Show-Error 'O executável atual foi preservado. Veja os detalhes em Atividade.'
        }
        return
    }

    if ($job.RoutePath) {
        if ($job.RoutePath -eq $ui.GamePath.Text.Trim()) {
            $route = $job.Output.ToString().Trim()
            $state.Route = if ($code -eq 0 -and $route -in @('optical-installed', 'feeder', 'feeder-new', 'feeder-dx11', 'feeder-ats-dx11', 'feeder-ats-overlay', 'feeder-unknown-api', 'feeder-overlay', 'feeder-overlay-dx11', 'opti', 'opti-dx11')) { $route } else { '' }
        }
        if ($state.Route -eq 'optical-installed') { $ui.OpticalF5.IsChecked = $true }
        Update-Controls
        return
    }

    if ($job.GameLibrary) {
        try {
            if ($code -ne 0) { throw 'A detecção de jogos não foi concluída.' }
            # Windows PowerShell 5.1 returns a JSON array as one pipeline item.
            [object[]]$games = ConvertFrom-Json -InputObject $job.Output.ToString()
            foreach ($game in $games) { Set-GameArtwork $game }
            $state.Games = $games
            $state.LibraryError = $false
        } catch {
            $state.LibraryError = $true
            Show-Error $_.Exception.GetBaseException().Message
        }
        Update-Controls
        Update-Library
        return
    }

    if ($code -eq 0) {
        if ($job.PreviewPath) { $state.PreviewedPath = $job.PreviewPath }
        $result = if ($job.KitDownload) { 'Kit completo pronto para usar' } else { 'Concluído' }
        $tone = 'Accent'
    } elseif ($job.Optimization -and $code -eq 3) {
        $result = 'Nenhum ajuste aplicável'
        $tone = 'Muted'
    } elseif ($job.Diagnostic -and $code -eq 1) {
        $result = 'Diagnóstico concluído com pendências'
        $tone = 'Warning'
    } else {
        $result = "Falha no comando (código $code)"
        $tone = 'Failure'
    }
    Add-Activity "  $result$( [Environment]::NewLine )"
    $ui.Status.Text = "$($job.Label) · $result"
    Set-OperationCard $job.Label $result $tone
    Update-Controls
    if ($job.PreviewPath) {
        $ui.Activity.UpdateLayout()
        $line = $ui.Activity.GetLineIndexFromCharacterIndex($job.LogStart)
        if ($line -ge 0) { $ui.Activity.ScrollToLine($line) }
    }

    if ($job.RefreshRoute) { Schedule-RouteCheck }

})

$routeTimer = [Windows.Threading.DispatcherTimer]::new()
$routeTimer.Interval = [TimeSpan]::FromMilliseconds(250)
$routeTimer.Add_Tick({
    if ($state.Job) { return }
    $routeTimer.Stop()
    $target = $ui.GamePath.Text.Trim()
    if ($target) {
        $routeArgs = @('dlss5', '--game', $target, '--route', '--route-details')
        if ($ui.UseFeeder.IsChecked) { $routeArgs += '--feeder' }
        Start-Operation 'Identificação da rota' $forge $routeArgs -RoutePath $target
    }
})

function Schedule-RouteCheck {
    $routeTimer.Stop()
    $state.Route = ''
    if (-not [string]::IsNullOrWhiteSpace($ui.GamePath.Text) -and [IO.File]::Exists($forge)) {
        $state.Route = 'pending'
        $routeTimer.Start()
    }
    Update-Controls
}

function Select-FolderForExe {
    if ([IO.Directory]::Exists($ui.GamePath.Text)) { return $ui.GamePath.Text }
    if ([IO.File]::Exists($ui.GamePath.Text)) { return [IO.Path]::GetDirectoryName($ui.GamePath.Text) }
}

function Set-GameArtwork($Game) {
    $artwork = $null
    $icon = $null
    if ($Game.Cover) {
        try {
            $picture = [Windows.Media.Imaging.BitmapImage]::new()
            $picture.BeginInit()
            $picture.DecodePixelWidth = 320
            $picture.CacheOption = 'OnLoad'
            $picture.UriSource = [Uri]::new($Game.Cover, [UriKind]::Absolute)
            $picture.EndInit()
            $artwork = $picture
        } catch {
            # The card already has a placeholder beneath the image.
        }
    }
    if ([IO.File]::Exists($Game.Target)) {
        try {
            $fileIcon = [Drawing.Icon]::ExtractAssociatedIcon($Game.Target)
            if ($fileIcon) {
                try {
                    $icon = [Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon($fileIcon.Handle,
                        [Windows.Int32Rect]::Empty, [Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions())
                } finally { $fileIcon.Dispose() }
            }
        } catch {
            # Some executables have no readable icon.
        }
    }
    Add-Member -InputObject $Game -NotePropertyName Artwork -NotePropertyValue $artwork
    Add-Member -InputObject $Game -NotePropertyName Icon -NotePropertyValue $icon
}

function Update-Library {
    $term = $ui.LibrarySearch.Text.Trim()
    $visible = @($state.Games | Where-Object {
        $_.Name.IndexOf($term, [StringComparison]::CurrentCultureIgnoreCase) -ge 0 -or
        $_.Store.IndexOf($term, [StringComparison]::CurrentCultureIgnoreCase) -ge 0
    })
    $ui.LibraryTiles.ItemsSource = $visible
    $ui.LibraryStatus.Text = if ($state.Job -and $state.Job.GameLibrary) { 'Buscando jogos instalados…' }
        elseif ($state.LibraryError) { 'Falha ao atualizar a biblioteca. Tente novamente ou selecione manualmente.' }
        elseif (!$state.Games.Count) { 'Nenhum jogo encontrado na Steam, Epic Games ou GOG.' }
        elseif (!$visible.Count) { 'Nenhum jogo corresponde à busca.' }
        elseif ($term) { "$($visible.Count) de $($state.Games.Count) jogos" }
        elseif ($visible.Count -eq 1) { '1 jogo instalado' }
        else { "$($visible.Count) jogos instalados" }
}

function Get-GameTarget {
    $inputPath = $ui.GamePath.Text.Trim()
    try {
        $item = Get-Item -LiteralPath $inputPath
        if ($item -is [IO.DirectoryInfo]) { return $item.FullName }
        if ($item -is [IO.FileInfo] -and $item.Extension -eq '.exe') { return $item.FullName }
    } catch {
        # The same message covers missing, inaccessible, and invalid paths.
    }
    Show-Error 'Escolha uma pasta válida ou informe o caminho do executável do jogo.'
}

function Invoke-GameOperation([string]$Action) {
    $target = Get-GameTarget
    if (-not $target) { return }
    $gameArgs = @('dlss5', '--game', $target)
    if ($ui.UseFeeder.IsChecked) { $gameArgs += '--feeder' }
    $oneClick = Get-OneClickPath
    if ($oneClick) { $gameArgs += @('--oneclick', $oneClick) }
    $useOverlay = $state.Route -in @('opti', 'feeder-overlay', 'feeder-overlay-dx11', 'feeder-ats-overlay') -or ($state.Route -in @('feeder', 'feeder-dx11', 'feeder-ats-dx11') -and $ui.UseInGameMenu.IsChecked)
    if ($Action -in @('Preview', 'Apply') -and $useOverlay) {
        if ($state.Route -ne 'opti-dx11') { $gameArgs += '--in-game-menu' }
        $upscaler = $ui.Upscaler.SelectedItem.Tag
        $frameGen = $ui.FrameGen.SelectedItem.Tag
        if ($upscaler) { $gameArgs += @('--upscaler', $upscaler) }
        if ($frameGen -and $state.Route -notin @('feeder-ats-dx11', 'feeder-ats-overlay')) {
            $gameArgs += @('--framegen', $frameGen)
            if ($frameGen -ne 'off') { $gameArgs += @('--fg-input', $ui.FrameGenInput.SelectedItem.Tag) }
        }
    }
    $optical = $ui.OpticalF5.IsChecked -or $state.Route -eq 'optical-installed'
    if ($optical -and $Action -in @('Preview','Apply')) {
        $gameArgs = @('dlss5','--game',$target,'--optical-f5','--optical-generator',$ui.OpticalGenerator.SelectedItem.Tag)
    }
    switch ($Action) {
        'Inventory' { Start-Operation 'Inventário de DLLs' $forge @('dlls', '--game', $target) }
        'Preview' {
            $state.PreviewedPath = $null
            Start-Operation 'Prévia da instalação' $forge ($gameArgs + '--dry-run') -PreviewPath $target
        }
        'Apply' {
            if ($state.PreviewedPath -ne $target) {
                $state.PreviewedPath = $null
                Show-Error 'Gere uma nova prévia para esta pasta antes de aplicar.'
                return
            }
            $message = if ($optical) {
                'Aplicar Optical F5Low ao executável selecionado? Feche o jogo antes de continuar.'
            } elseif ($useOverlay -and $state.Route -in @('feeder', 'feeder-dx11', 'feeder-ats-dx11', 'feeder-overlay', 'feeder-overlay-dx11', 'feeder-ats-overlay')) {
                'Preparar o menu do OptiScaler no jogo? O Forge guarda o RenoDX para poder restaurá-lo. Feche o jogo antes de continuar.'
            } else {
                'Aplicar a instalação da prévia? Feche o jogo antes de continuar.'
            }
            $message += "$([Environment]::NewLine)$([Environment]::NewLine)Jogo selecionado: $target"
            if ([Windows.MessageBox]::Show($window, $message, 'Aplicar kit DLSS 5', 'YesNo', 'Warning', 'No') -eq 'Yes') {
                $state.PreviewedPath = $null
                Start-Operation 'Instalação do kit' $forge ($gameArgs + '--yes')
            }
        }
        'Optimize' {
            $state.PreviewedPath = $null
            Start-Operation 'Otimização para este PC' $forge @('dlss5', '--game', $target, '--optimize')
        }
        'RestoreResolution' {
            Start-Operation 'Restauração da resolução' $forge @('dlss5', '--game', $target, '--restore-resolution')
        }
        'Undo' {
            $message = if ($state.Route -eq 'optical-installed') {
                'Remover Optical F5Low e restaurar a configuração anterior?'
            } elseif ($state.Route -in @('feeder-overlay', 'feeder-overlay-dx11', 'feeder-ats-overlay')) {
                'Remover o menu OptiScaler e voltar ao RenoDX no Feeder?'
            } else {
                'Remover a instalação do jogo? Na rota Feeder, o DLSS5oneclick remove os componentes neurais e pode manter o ReShade.'
            }
            $message += "$([Environment]::NewLine)$([Environment]::NewLine)Jogo selecionado: $target"
            if ([Windows.MessageBox]::Show($window, $message, 'Desfazer instalação', 'YesNo', 'Warning', 'No') -eq 'Yes') {
                $state.PreviewedPath = $null
                Start-Operation 'Restauração do jogo' $forge ($gameArgs + '--undo')
            }
        }
    }
}

$ui.Browse.Add_Click({
    $dialog = [System.Windows.Forms.FolderBrowserDialog]::new()
    try {
        $dialog.Description = 'Escolha a pasta que contém o executável do jogo.'
        $dialog.ShowNewFolderButton = $false
        if ([IO.Directory]::Exists($ui.GamePath.Text)) { $dialog.SelectedPath = $ui.GamePath.Text }
        if ($dialog.ShowDialog() -eq 'OK') { $ui.GamePath.Text = $dialog.SelectedPath }
    } finally {
        $dialog.Dispose()
    }
})
$ui.BrowseExe.Add_Click({
    $dialog = [Microsoft.Win32.OpenFileDialog]::new()
    $dialog.Title = 'Selecione o executável do jogo'
    $dialog.Filter = 'Executáveis (*.exe)|*.exe'
    $folder = Select-FolderForExe
    if ($folder) { $dialog.InitialDirectory = $folder }
    if ($dialog.ShowDialog($window) -eq $true) { $ui.GamePath.Text = $dialog.FileName }
})
function Scan-GameLibrary {
    if ($state.Job -or ![IO.File]::Exists($gameScan)) { return }
    Start-Operation 'Detecção de jogos' $psExe @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $gameScan) -GameLibrary
    Update-Library
}
$ui.FindGames.Add_Click({ Scan-GameLibrary })
$ui.LibrarySearch.Add_TextChanged({ Update-Library })
$ui.LibraryTiles.AddHandler([Windows.Controls.Primitives.ButtonBase]::ClickEvent, [Windows.RoutedEventHandler]{
    param($sender, $eventArgs)
    $selected = $eventArgs.OriginalSource.Tag
    if (!$selected.Target -or $state.Job) { return }
    if (![IO.Directory]::Exists($selected.Folder)) {
        Scan-GameLibrary
        return
    }
    $state.SelectedGame = $selected
    $ui.GamePath.Text = $selected.Target
    $ui.ManualTarget.IsExpanded = $false
    Show-ForgePage 'GamePage' -Details
    Update-Controls
    $ui.Status.Text = "$($selected.Name) selecionado"
})
$ui.ManualGame.Add_Click({
    Show-ForgePage 'GamePage' -Details
    $ui.ManualTarget.IsExpanded = $true
    [void]$ui.GamePath.Focus()
})
$ui.BackToLibrary.Add_Click({ Show-ForgePage 'GamePage' })
$ui.ChooseOneClick.Add_Click({
    $dialog = [Microsoft.Win32.OpenFileDialog]::new()
    $dialog.Title = 'Selecione o executável do DLSS5oneclick'
    $dialog.Filter = 'DLSS5oneclick (dlss5oneclick*.exe)|dlss5oneclick*.exe|Executáveis (*.exe)|*.exe'
    if ($dialog.ShowDialog($window) -eq $true) {
        $state.OneClick = $dialog.FileName
        $state.PreviewedPath = $null
        try {
            [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($oneClickPreference))
            [IO.File]::WriteAllText($oneClickPreference, $state.OneClick)
        } catch {
            Add-Activity 'O vínculo do OneClick vale para esta sessão. Não foi possível salvá-lo.'
        }
        Update-Controls
    }
})
$ui.OneClickReleases.Add_Click({
    Start-Process 'https://github.com/faisalkindi/DLSS5oneclick/releases'
})
foreach ($name in @('Upscaler', 'FrameGen', 'FrameGenInput')) {
    $ui[$name].Add_SelectionChanged({ $state.PreviewedPath = $null; Update-Controls })
}
$ui.OpticalF5.Add_Checked({ $state.PreviewedPath = $null; Update-Controls })
$ui.OpticalF5.Add_Unchecked({ $state.PreviewedPath = $null; Update-Controls })
$ui.OpticalGenerator.Add_SelectionChanged({ $state.PreviewedPath = $null; Update-Controls })
$ui.UseInGameMenu.Add_Checked({ $state.PreviewedPath = $null; Update-Controls })
$ui.UseInGameMenu.Add_Unchecked({ $state.PreviewedPath = $null; Update-Controls })
$ui.UseFeeder.Add_Checked({ $state.PreviewedPath = $null; Schedule-RouteCheck })
$ui.UseFeeder.Add_Unchecked({ $state.PreviewedPath = $null; Schedule-RouteCheck })
$ui.GamePath.Add_TextChanged({
    if ($state.SelectedGame -and $state.SelectedGame.Target -ne $ui.GamePath.Text) { $state.SelectedGame = $null }
    $ui.UseInGameMenu.IsChecked = $false
    $ui.Upscaler.SelectedIndex = 0
    $ui.FrameGen.SelectedIndex = 0
    $ui.FrameGenInput.SelectedIndex = 0
    $state.PreviewedPath = $null
    Schedule-RouteCheck
})
function Set-LogExpanded([bool]$Expanded) {
    $ui.WorkScroll.Visibility = if ($Expanded) { 'Collapsed' } else { 'Visible' }
    [Windows.Controls.Grid]::SetColumn($ui.ActivityPane, $(if ($Expanded) { 0 } else { 2 }))
    [Windows.Controls.Grid]::SetColumnSpan($ui.ActivityPane, $(if ($Expanded) { 3 } else { 1 }))
    $ui.ExpandLog.Content = if ($Expanded) { 'Voltar' } else { 'Ampliar' }
}

function Show-ForgePage([string]$Page, [switch]$Details) {
    Set-LogExpanded $false
    foreach ($name in @('GamePage', 'PcPage', 'ToolsPage')) {
        $ui[$name].Visibility = if ($name -eq $Page) { 'Visible' } else { 'Collapsed' }
    }
    $library = $Page -eq 'GamePage' -and !$Details
    $ui.LibraryView.Visibility = if ($library) { 'Visible' } else { 'Collapsed' }
    $ui.GameDetails.Visibility = if ($Page -eq 'GamePage' -and $Details) { 'Visible' } else { 'Collapsed' }
    $ui.ActivityPane.Visibility = if ($library) { 'Collapsed' } else { 'Visible' }
    [Windows.Controls.Grid]::SetColumnSpan($ui.WorkScroll, $(if ($library) { 3 } else { 1 }))
    $ui.InstallBar.Visibility = $ui.GameDetails.Visibility
    $ui.WorkScroll.ScrollToHome()
}
$ui.NavGames.Add_Checked({ Show-ForgePage 'GamePage' })
$ui.NavPc.Add_Checked({ Show-ForgePage 'PcPage' })
$ui.NavTools.Add_Checked({ Show-ForgePage 'ToolsPage' })

$ui.Inventory.Add_Click({ Invoke-GameOperation 'Inventory' })
$ui.Preview.Add_Click({ Invoke-GameOperation 'Preview' })
$ui.Apply.Add_Click({ Invoke-GameOperation 'Apply' })
$ui.Undo.Add_Click({ Invoke-GameOperation 'Undo' })
$ui.Optimize.Add_Click({ Invoke-GameOperation 'Optimize' })
$ui.RestoreResolution.Add_Click({ Invoke-GameOperation 'RestoreResolution' })
$ui.Detect.Add_Click({ Start-Operation 'Detecção de hardware' $forge @('detect') })
$ui.Plan.Add_Click({ Start-Operation 'Plano do PC' $forge @('plan') })
$ui.Doctor.Add_Click({ Start-Operation 'Diagnóstico do PC' $forge @('doctor') -Diagnostic })
$ui.Download.Add_Click({
    $state.PreviewedPath = $null
    Start-Operation 'Preparação do kit completo' $psExe @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
        $kitScript, '-Dir', $kitFolder, '-Complete')
})
$ui.ImportModel.Add_Click({
    $dialog = [Microsoft.Win32.OpenFileDialog]::new()
    $dialog.Title = 'Selecione o modelo NVIDIA DLSS Neural Rendering'
    $dialog.Filter = 'Modelo NVIDIA (nvngx_dlssnr.dll)|nvngx_dlssnr.dll'
    if ($dialog.ShowDialog($window) -eq $true) {
        $state.PreviewedPath = $null
        Start-Operation 'Importação do modelo NVIDIA' $psExe @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $kitScript, '-Dir', $kitFolder, '-Model', $dialog.FileName, '-ImportOnly')
    }
})
$ui.Build.Add_Click({
    $state.PreviewedPath = $null
    Start-Operation 'Compilação nativa' $psExe @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $buildScript)
})
$ui.Menu.Add_Click({
    try {
        Start-Process -FilePath $psExe -WorkingDirectory $root -ArgumentList (
            Join-WindowsArguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $menuScript))
    } catch {
        Show-Error "Falha ao abrir o menu CLI. $($_.Exception.GetBaseException().Message)"
    }
})
$ui.CopyLog.Add_Click({
    try {
        [Windows.Clipboard]::SetText($ui.Activity.Text)
        if ($null -eq $state.Job) { $ui.Status.Text = 'Saída copiada.' }
    } catch {
        $ui.Status.Text = 'Área de transferência ocupada. Tente copiar novamente.'
    }
})
$ui.ExpandLog.Add_Click({ Set-LogExpanded ($ui.WorkScroll.Visibility -eq 'Visible') })
$ui.ClearLog.Add_Click({
    $ui.Activity.Clear()
    $state.PreviewedPath = $null
    $ui.Status.Text = 'Painel limpo.'
    $ui.OperationCard.Visibility = 'Collapsed'
    Update-Controls
})
if ($appVersion -match '^\d+\.\d+\.\d+$') {
    $ui.AppUpdate.Visibility = 'Visible'
    $ui.AppUpdateButton.Add_Click({
        if (!$updateState.Tag) { Start-UpdateCheck -Manual; return }
        Start-Operation "Atualização $($updateState.Tag)" $psExe @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $updateScript,
            '-Action', 'Stage', '-Executable', $forge, '-CurrentVersion', $appVersion, '-ReleaseTag', $updateState.Tag)
    })
    $updateTimer = [Windows.Threading.DispatcherTimer]::new()
    $updateTimer.Interval = [TimeSpan]::FromSeconds(1)
    $updateTimer.Add_Tick({
        $check = $updateState.Check
        if (!$check) {
            if ([datetime]::UtcNow -ge $updateState.NextCheck) { Start-UpdateCheck }
            return
        }
        if (!$check.Process.HasExited -or !$check.Output.IsCompleted -or !$check.ErrorOutput.IsCompleted) { return }
        $tag = $check.Output.Result.Trim()
        $code = $check.Process.ExitCode
        $check.Process.Dispose()
        $updateState.Check = $null
        $updateState.Tag = ''
        if ($code -eq 0 -and $tag -match '^v\d+\.\d+\.\d+$') {
            $updateState.Tag = $tag
            Set-AppUpdateStatus 'Atualização disponível' 'Accent'
        } elseif ($code -eq 0) {
            Set-AppUpdateStatus $(if ($updateState.ManualCheck) { 'Versão atual' } else { '' })
        } else {
            Set-AppUpdateStatus 'Falha na verificação' 'Warning' $check.ErrorOutput.Result.Trim()
        }
        Update-Controls
    })
    $window.Add_ContentRendered({ $updateTimer.Start() })
    $window.Add_Closed({ $updateTimer.Stop() })
}
$window.Add_Closing({
    if ($null -ne $state.Job) {
        $_.Cancel = $true
        $ui.Status.Text = "$($state.Job.Label) em andamento. Aguarde para fechar."
    }
})

if ($NoTitleBar) { $window.WindowStyle = 'None' }
if ([IO.File]::Exists($oneClickPreference)) {
    try {
        $savedOneClick = [IO.File]::ReadAllText($oneClickPreference).Trim()
        if ([IO.File]::Exists($savedOneClick)) { $state.OneClick = $savedOneClick }
    } catch {
        # An unavailable preference must not stop the interface from opening.
    }
}
$ui.GamePath.Text = $Path
if ($Path) {
    Show-ForgePage 'GamePage' -Details
    $ui.ManualTarget.IsExpanded = $true
} else { Show-ForgePage 'GamePage' }
$firstScan = [Windows.Threading.DispatcherTimer]::new()
$firstScan.Interval = [TimeSpan]::FromMilliseconds(200)
$firstScan.Add_Tick({
    if ($state.Job) { return }
    $firstScan.Stop()
    Scan-GameLibrary
})
$window.Add_ContentRendered({ $firstScan.Start() })
$ui.Status.Text = 'Pronto'
Update-Controls
[void]$window.ShowDialog()
