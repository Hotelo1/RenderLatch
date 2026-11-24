# Testa o menu de terminal (envia 0 para fechar).
$root = Split-Path $PSScriptRoot -Parent
$p = New-Object System.Diagnostics.ProcessStartInfo (Join-Path $root 'forge.exe')
$p.Arguments = 'menu'
$p.WorkingDirectory = $root
$p.RedirectStandardInput = $true
$p.RedirectStandardOutput = $true
$p.RedirectStandardError = $true
$p.UseShellExecute = $false
$proc = [System.Diagnostics.Process]::Start($p)
$proc.StandardInput.WriteLine('0')
$proc.StandardInput.Close()
$stdout = $proc.StandardOutput.ReadToEndAsync()
$stderr = $proc.StandardError.ReadToEndAsync()
$proc.WaitForExit()
Write-Output "exit=$($proc.ExitCode)"
Write-Output '--- stdout ---'
$out = $stdout.Result
$out.Split("`n") | Select-Object -First 24
Write-Output '--- stderr ---'
$err = $stderr.Result
$err.Split("`n") | Select-Object -First 10
