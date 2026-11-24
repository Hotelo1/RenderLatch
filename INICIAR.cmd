@echo off
setlocal
title RenderLatch
cd /d "%~dp0"
if exist "%~dp0forge.exe" (
  "%~dp0forge.exe" gui
) else (
  powershell -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0tools\gui.ps1"
)
if errorlevel 1 (
  echo.
  echo   Algo deu errado. Rode manualmente:
  echo     powershell -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0tools\gui.ps1"
  pause
)
