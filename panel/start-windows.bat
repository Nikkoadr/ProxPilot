@echo off
REM ProxPilot Panel — Windows dev runner (tanpa install.sh, tanpa git).
REM Usage: double-klik atau: start-windows.bat
REM Env (opsional): set PORT=8080 & set PANEL_DATA=./data
REM
REM Alur dev: edit kode -> jalankan file ini -> buka browser.
REM Git + tag + panel-update hanya untuk update server produksi.
cd /d "%~dp0"

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Rust belum terinstal.
  echo Install rustup-init.exe dari https://rust-lang.org/tools/install/ lalu buka terminal baru.
  exit /b 1
)
if "%PORT%"=="" set PORT=8080
if "%PANEL_DATA%"=="" set PANEL_DATA=./data
REM CARGO_TARGET_DIR default (target/). WSL memakai target-linux (lihat start.sh),
REM jadi build Windows dan WSL tidak saling timpa.

where wsl >nul 2>&1
if errorlevel 1 (
  echo WSL tidak ditemukan - mode windows-native.
  echo Butuh terraform.exe di PATH untuk local mode; ansible tidak tersedia di Windows.
) else (
  if "%PANEL_WSL_DISTRO%"=="" ( set WSLD= ) else ( set WSLD=-d %PANEL_WSL_DISTRO% )
  wsl %WSLD% bash -lc "exit 0" >nul 2>&1
  if errorlevel 1 (
    echo WSL bermasalah: distro default mungkin bukan Ubuntu.
    echo Perbaiki sekali saja, pilih satu:
    echo   wsl --set-default Ubuntu
    echo   atau: set PANEL_WSL_DISTRO=Ubuntu
  ) else (
    echo WSL bridge: OK - terraform/ansible/ssh dijalankan lewat WSL otomatis.
  )
)

echo Panel dev: http://localhost:%PORT%  (data: %PANEL_DATA%\panel.db, login admin/admin123)
cargo run
