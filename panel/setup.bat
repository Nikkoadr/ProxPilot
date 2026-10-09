@echo off
REM ProxPilot panel Go - build + jalan NATIVE Windows, localhost stabil.
REM Usage: setup.bat [PORT], default 8080
REM Catatan: fitur Ansible Configure + Salin-Key SSH tetap butuh WSL/Linux.
setlocal
set DISTRO=Ubuntu
set PORT=%1
if "%PORT%"=="" set PORT=8080

where wsl >nul 2>&1
if errorlevel 1 goto nowsl

echo === Build panel, sekali saja, cache berikutnya ===
wsl -d %DISTRO% bash -c "export GOROOT=$HOME/go GOPATH=$HOME/gopath PATH=$HOME/go/bin:/usr/bin:/bin GOFLAGS=-mod=mod; command -v go >/dev/null || { echo 'Go belum ada, download...'; curl -sSL -o /tmp/go.tgz https://go.dev/dl/go1.24.6.linux-amd64.tar.gz && rm -rf $HOME/go && tar -C $HOME -xzf /tmp/go.tgz; }; cd /mnt/d/laragon/www/ProxPilot/panel && GOOS=windows GOARCH=amd64 go build -buildvcs=false -o bin/proxpilot.exe ."
if errorlevel 1 goto buildfail

echo.
echo === ProxPilot jalan di http://localhost:%PORT% ===
echo Tutup window ini untuk menghentikan panel.
echo.
cd /d "%~dp0"
set PANEL_DATA=.\data
bin\proxpilot.exe
echo.
echo --- Panel berhenti. ---
pause
exit /b 0

:nowsl
echo WSL tidak ditemukan. Install WSL2 + Ubuntu dulu untuk build.
pause
exit /b 1

:buildfail
echo.
echo --- Build GAGAL, lihat error di atas. ---
pause
exit /b 1
