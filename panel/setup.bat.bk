@echo off
REM ProxPilot panel Go - jalan di WSL via go run, seperti start.sh.
REM Usage: setup.bat [PORT], default 8080
setlocal
set DISTRO=Ubuntu
set PORT=%1
if "%PORT%"=="" set PORT=8080

where wsl >nul 2>&1
if errorlevel 1 goto nowsl

for /f "tokens=*" %%i in ('wsl -d %DISTRO% hostname -I') do set WSLIP=%%i
echo === ProxPilot di WSL ===
echo Buka salah satu:
echo   http://localhost:%PORT%
echo   http://%WSLIP%:%PORT%
echo Tutup window ini untuk menghentikan panel.
echo.

wsl -d %DISTRO% bash -c "export GOROOT=$HOME/go GOPATH=$HOME/gopath PATH=$HOME/go/bin:/usr/bin:/bin GOFLAGS='-mod=mod -buildvcs=false'; command -v go >/dev/null || { echo 'Go belum ada, download...'; curl -sSL -o /tmp/go.tgz https://go.dev/dl/go1.24.6.linux-amd64.tar.gz && rm -rf $HOME/go && tar -C $HOME -xzf /tmp/go.tgz; }; cd /mnt/d/laragon/www/ProxPilot/panel && PORT=%PORT% PANEL_DATA=./data go run ."
echo.
echo --- Panel berhenti. ---
pause
exit /b 0

:nowsl
echo WSL tidak ditemukan. Install WSL2 + Ubuntu dulu.
pause
exit /b 1
