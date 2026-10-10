@echo off
REM ProxPilot Panel Go — runner + tools.
REM Usage:
REM   setup.bat [PORT]   jalan panel, port otomatis cari yang bebas
REM   setup.bat check    diagnosa: WSL, Go, port, ansible, ssh key
REM   setup.bat deps     install ansible+sshpass+build-essential di WSL
REM   setup.bat update   git pull
REM Env opsional: set PORT=8080 ^& set PANEL_DATA=./data ^& set PANEL_WSL_DISTRO=Ubuntu
setlocal
for /F %%a in ('echo prompt $E^| cmd') do set "ESC=%%a"
set "C_RESET=%ESC%[0m"
set "C_BOLD=%ESC%[1m"
set "C_GREEN=%ESC%[32m"
set "C_YELLOW=%ESC%[33m"
set "C_RED=%ESC%[31m"
set "C_CYAN=%ESC%[36m"
set "C_GRAY=%ESC%[90m"

set MODE=run
set ARG=%1
if "%ARG%"=="check" set MODE=check
if "%ARG%"=="deps" set MODE=deps
if "%ARG%"=="update" set MODE=update
if "%ARG%"=="help" goto help
if "%ARG%"=="-h" goto help
if "%ARG%"=="--help" goto help

call :banner
call :step "Cek WSL"
where wsl >nul 2>&1
if errorlevel 1 goto nowsl
if "%PANEL_WSL_DISTRO%"=="" (
  set WSLD=
) else (
  set WSLD=-d %PANEL_WSL_DISTRO%
)
wsl %WSLD% bash -lc "exit 0" >nul 2>&1
if errorlevel 1 goto baddistro
for /f "delims=" %%i in ('wsl %WSLD% wslpath -a "%~dp0."') do set WSLDIR=%%i
call :ok "WSL siap"

if "%MODE%"=="check" goto docheck
if "%MODE%"=="deps" goto dodeps
if "%MODE%"=="update" goto doupdate

REM --- mode run: PORT bisa angka atau kosong ---
echo %ARG% | findstr /r "^[0-9][0-9]*$" >nul
if not errorlevel 1 set PORT=%ARG%
if "%PORT%"=="" set PORT=8080
if "%PANEL_DATA%"=="" set PANEL_DATA=./data
goto dorun

:dorun
call :step "Cek Go di WSL"
wsl %WSLD% bash -lc "command -v go || test -x $HOME/go/bin/go" >nul 2>&1
if errorlevel 1 (
  call :info "Go belum ada, install otomatis..."
  wsl %WSLD% bash -c "curl -sSL -o /tmp/go.tgz https://go.dev/dl/go1.24.6.linux-amd64.tar.gz && rm -rf $HOME/go && tar -C $HOME -xzf /tmp/go.tgz && $HOME/go/bin/go version"
  if errorlevel 1 goto gofail
)
call :ok "Go siap"
call :step "Cari port bebas"
set TRIES=0
:findport
wsl %WSLD% bash -c "ss -tln 2>/dev/null | grep -q ':%PORT% '" >nul 2>&1
if errorlevel 1 goto portok
set /a PORT+=1
set /a TRIES+=1
if %TRIES% GEQ 10 goto portfail
goto findport
:portok
call :ok "Port %PORT% bebas"
call :step "Build panel"
wsl %WSLD% bash -c "export GOROOT=$HOME/go GOPATH=$HOME/gopath PATH=$HOME/go/bin:$HOME/.local/bin:/usr/bin:/bin GOFLAGS='-mod=mod -buildvcs=false' CGO_ENABLED=0; cd %WSLDIR% && go build -buildvcs=false -o bin/proxpilot ."
if errorlevel 1 goto buildfail
call :ok "Build sukses"
for /f "tokens=*" %%i in ('wsl %WSLD% hostname -I') do set WSLIP=%%i
echo.
call :info "Panel jalan, buka salah satu:"
echo   %C_CYAN%http://localhost:%PORT%%C_RESET%
echo   %C_CYAN%http://%WSLIP%:%PORT%%C_RESET%
echo %C_GRAY%Tutup window ini untuk menghentikan panel.%C_RESET%
echo.
wsl %WSLD% bash -c "cd %WSLDIR% && PORT=%PORT% PANEL_DATA=%PANEL_DATA% ./bin/proxpilot"
echo.
echo --- Panel berhenti. ---
pause
exit /b 0

:docheck
call :step "Diagnosa ProxPilot"
wsl %WSLD% bash -c "command -v go >/dev/null && go version || { test -x $HOME/go/bin/go && $HOME/go/bin/go version || echo 'Go: BELUM ADA'; }; command -v ansible-playbook >/dev/null && ansible-playbook --version | head -1 || echo 'Ansible: BELUM ADA'; test -f $HOME/.ssh/id_ed25519.pub && echo 'SSH key: ADA' || echo 'SSH key: BELUM ADA'; ss -tln 2>/dev/null | grep -E ':808|:809' && echo '(port di atas sedang dipakai)' || echo 'Port 808x: bebas'"
pause
exit /b 0

:dodeps
call :step "Install deps WSL"
echo Butuh password sudo sekali.
wsl %WSLD% bash -c "sudo apt update && sudo apt install -y ansible sshpass build-essential"
wsl %WSLD% bash -c "mkdir -p $HOME/.local/bin && curl -sSL -o /tmp/tf.zip https://releases.hashicorp.com/terraform/1.9.8/terraform_1.9.8_linux_amd64.zip && python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' /tmp/tf.zip $HOME/.local/bin && $HOME/.local/bin/terraform version | head -1"
pause
exit /b 0

:doupdate
call :step "Update repo"
where git >nul 2>&1
if not errorlevel 1 (
  git pull
) else (
  wsl %WSLD% bash -c "cd %WSLDIR%/.. && git pull"
)
pause
exit /b 0

:banner
echo %C_CYAN%============================================%C_RESET%
echo %C_CYAN%  ProxPilot Panel - Setup%C_RESET%
echo %C_CYAN%============================================%C_RESET%
echo.
exit /b 0

:step
set "_m=%~1"
echo %C_BOLD%%_m% ...%C_RESET%
exit /b 0

:ok
set "_m=%~1"
echo %C_GREEN%  [OK] %_m%%C_RESET%
exit /b 0

:info
set "_m=%~1"
echo %C_YELLOW%  --^> %_m%%C_RESET%
exit /b 0

:nowsl
echo %C_RED%WSL tidak ditemukan. Install WSL2 + Ubuntu dulu.%C_RESET%
pause
exit /b 1

:baddistro
echo %C_RED%WSL bermasalah: distro default mungkin bukan Ubuntu.%C_RESET%
echo Perbaiki sekali saja, pilih satu:
echo   wsl --set-default Ubuntu
echo   atau: set PANEL_WSL_DISTRO=Ubuntu
pause
exit /b 1

:gofail
echo %C_RED%ERROR: install Go otomatis gagal, butuh internet.%C_RESET%
pause
exit /b 1

:portfail
echo %C_RED%10 port berturut-turut sibuk, pilih manual: setup.bat 8090%C_RESET%
pause
exit /b 1

:buildfail
echo %C_RED%Build GAGAL, lihat error di atas.%C_RESET%
pause
exit /b 1

:help
echo Usage: setup.bat [PORT ^| check ^| deps ^| update]
pause
exit /b 0
