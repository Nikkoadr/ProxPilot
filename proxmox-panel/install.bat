@echo off
REM Proxmox Panel — Windows installer (binary + autostart).
REM Usage: set REPO=owner/repo first, then run as Administrator for autostart.
REM   set REPO=myorg/proxmox-panel
REM   install.bat
chcp 65001 >nul
setlocal

if "%REPO%"=="" (
    echo ERROR: set REPO=owner/repo to your GitHub releases repo first.
    exit /b 1
)
if "%VERSION%"=="" set VERSION=latest

set APPDIR=%ProgramFiles%\ProxmoxPanel
set DATA=%ProgramData%\ProxmoxPanel

echo [1/4] Downloading binary (%REPO% @ %VERSION%)...
if not exist "%APPDIR%" mkdir "%APPDIR%"
if not exist "%DATA%" mkdir "%DATA%"

powershell -NoProfile -Command ^
  "$tag='%VERSION%';" ^
  "if($tag -eq 'latest'){ $tag=(Invoke-RestMethod \"https://api.github.com/repos/%REPO%/releases/latest\").tag_name };" ^
  "Invoke-WebRequest \"https://github.com/%REPO%/releases/download/$tag/proxmox-panel-windows-x86_64.exe\" -OutFile \"%APPDIR%\proxmox-panel.exe\""
if errorlevel 1 (
    echo ERROR: download failed. Check REPO/VERSION and release asset names.
    exit /b 1
)

echo [2/4] Data dir: %DATA%
echo NOTE: Linux deps (ansible/terraform) still live in WSL. In WSL run:
echo   curl -fsSL https://raw.githubusercontent.com/%REPO%/main/proxmox-panel/install.sh ^| bash
echo   (set PANEL_REPO=%REPO% first:  PANEL_REPO=%REPO% bash)

echo [3/4] Autostart (Scheduled Task: ProxmoxPanel, on logon)...
schtasks /create /tn "ProxmoxPanel" /tr "\"%APPDIR%\proxmox-panel.exe\"" /sc onlogon /rl highest /f
if errorlevel 1 (
    echo WARN: schtasks failed (run as Administrator to enable autostart).
) else (
    echo Starting now...
    schtasks /run /tn "ProxmoxPanel" >nul 2>&1
)

echo [4/4] Done.
echo   URL   : http://localhost:8080
echo   Login : admin / admin123 (change in Settings!)
echo   Data  : %DATA%\panel.db  ^(set PANEL_DATA to override^)
endlocal
