@echo off
REM ProxPilot Panel - Windows dev runner untuk Go via WSL.
REM Usage: double-klik atau setup.bat [PORT]
REM Env opsional: PORT, PANEL_DATA, PANEL_WSL_DISTRO
REM Tidak membutuhkan install.sh atau Git.

setlocal
cd /d "%~dp0"

REM Default konfigurasi
if "%PORT%"=="" set "PORT=8080"
if "%PANEL_DATA%"=="" set "PANEL_DATA=./data"
if "%PANEL_WSL_DISTRO%"=="" set "PANEL_WSL_DISTRO=Ubuntu"

REM Periksa WSL
where wsl >nul 2>&1
if errorlevel 1 (
    echo ERROR: WSL tidak ditemukan.
    echo Install WSL2 dan Ubuntu terlebih dahulu.
    exit /b 1
)

REM Periksa apakah distro dapat dijalankan
wsl -d "%PANEL_WSL_DISTRO%" bash -lc "exit 0" >nul 2>&1
if errorlevel 1 (
    echo ERROR: WSL distro "%PANEL_WSL_DISTRO%" tidak dapat dijalankan.
    echo Periksa dengan perintah: wsl -l -v
    echo Atur distro melalui PANEL_WSL_DISTRO jika diperlukan.
    exit /b 1
)

echo.
echo === ProxPilot Go Dev Runner ===
echo Distro : %PANEL_WSL_DISTRO%
echo Port   : %PORT%
echo Data   : %PANEL_DATA%
echo.
echo Buka browser: http://localhost:%PORT%
echo Tekan Ctrl+C untuk menghentikan panel.
echo.

REM Jalankan Go di WSL, menggunakan direktori file BAT ini.
wsl -d "%PANEL_WSL_DISTRO%" bash -lc "set -eu; export GOPATH=\$HOME/gopath; export PATH=\$GOPATH/bin:/usr/local/go/bin:\$HOME/.local/opt/go1.24.6/bin:/usr/bin:/bin; export GOFLAGS='-mod=mod -buildvcs=false'; if ! command -v go >/dev/null 2>&1; then echo 'Go belum tersedia, mengunduh Go 1.24.6...'; command -v curl >/dev/null 2>&1 || { echo 'ERROR: curl belum terpasang.'; exit 1; }; mkdir -p \"\$HOME/.local/opt/go1.24.6\"; curl -fSL --retry 3 https://go.dev/dl/go1.24.6.linux-amd64.tar.gz -o /tmp/proxpilot-go1.24.6.tar.gz; tar -xzf /tmp/proxpilot-go1.24.6.tar.gz --strip-components=1 -C \"\$HOME/.local/opt/go1.24.6\"; export PATH=\$HOME/.local/opt/go1.24.6/bin:\$PATH; fi; PROJECT_DIR=\$(wslpath -a '%CD%'); cd \"\$PROJECT_DIR\"; echo 'Menjalankan ProxPilot...'; PORT='%PORT%' PANEL_DATA='%PANEL_DATA%' go run ."

if errorlevel 1 (
    echo.
    echo ERROR: Panel gagal dijalankan atau berhenti karena error.
    pause
    exit /b 1
)

echo.
echo --- Panel berhenti. ---
pause
exit /b 0
