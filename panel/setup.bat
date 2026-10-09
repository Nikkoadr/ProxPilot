@echo off
REM ProxPilot Panel Go — Windows dev runner, tanpa install.sh, tanpa git.
REM Usage: double-klik atau: setup.bat
REM Env opsional: set PORT=8080 ^& set PANEL_DATA=./data
REM
REM Alur dev: edit kode -^> jalankan file ini -^> buka browser.
cd /d "%~dp0"

where wsl >nul 2>&1
if errorlevel 1 (
  echo ERROR: WSL tidak ditemukan.
  echo Install WSL2 + Ubuntu dari Microsoft Store lalu buka file ini lagi.
  pause
  exit /b 1
)
if "%PANEL_WSL_DISTRO%"=="" (
  set WSLD=
) else (
  set WSLD=-d %PANEL_WSL_DISTRO%
)
wsl %WSLD% bash -lc "exit 0" >nul 2>&1
if errorlevel 1 (
  echo WSL bermasalah: distro default mungkin bukan Ubuntu.
  echo Perbaiki sekali saja, pilih satu:
  echo   wsl --set-default Ubuntu
  echo   atau: set PANEL_WSL_DISTRO=Ubuntu
  pause
  exit /b 1
)
wsl %WSLD% bash -lc "command -v go || test -x $HOME/go/bin/go" >nul 2>&1
if errorlevel 1 (
  echo Go belum ada di WSL, install otomatis...
  wsl %WSLD% bash -c "curl -sSL -o /tmp/go.tgz https://go.dev/dl/go1.24.6.linux-amd64.tar.gz && rm -rf $HOME/go && tar -C $HOME -xzf /tmp/go.tgz && $HOME/go/bin/go version"
  if errorlevel 1 (
    echo ERROR: install Go otomatis gagal.
    echo Manual di terminal Ubuntu:
    echo   curl -sSL -o /tmp/go.tgz https://go.dev/dl/go1.24.6.linux-amd64.tar.gz
    echo   mkdir -p $HOME/go ^&^& tar -C $HOME -xzf /tmp/go.tgz
    pause
    exit /b 1
  )
)
if "%PORT%"=="" set PORT=8080
if "%PANEL_DATA%"=="" set PANEL_DATA=./data
for /f "delims=" %%i in ('wsl %WSLD% wslpath -a "%~dp0."') do set WSLDIR=%%i

echo WSL bridge: OK - terraform/ansible/ssh dijalankan lewat WSL otomatis.
echo Panel dev: http://localhost:%PORT%  (data: %PANEL_DATA%\panel_go.db, login admin/admin123)
wsl %WSLD% bash -c "export GOROOT=$HOME/go GOPATH=$HOME/gopath PATH=$HOME/go/bin:/usr/bin:/bin GOFLAGS='-mod=mod -buildvcs=false' CGO_ENABLED=0; cd %WSLDIR% && PORT=%PORT% PANEL_DATA=%PANEL_DATA% go run ."
echo.
echo --- Panel berhenti. ---
pause
