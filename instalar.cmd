@echo off
setlocal
cd /d "%~dp0"
title DLSS5 Forge - instalacao
echo.
echo   DLSS5 FORGE
echo   Compilando para ESTE PC. Nao feche a janela.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1"
if errorlevel 1 (
  echo.
  echo   Falhou. Veja a mensagem acima.
  pause
  exit /b 1
)
echo.
echo   ------------------------------------------------------------
echo   Diagnostico da sua maquina
echo   ------------------------------------------------------------
".\forge.exe" doctor
echo.
pause
echo.
echo   ------------------------------------------------------------
echo   Abrindo o menu.
echo   Opcao 8 baixa o kit da DLSS 5; opcao 9 aplica num jogo.
echo   ------------------------------------------------------------
".\forge.exe" menu
