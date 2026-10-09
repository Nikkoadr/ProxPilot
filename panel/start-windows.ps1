# ProxPilot Panel — Windows dev runner (native, tanpa install.sh).
# Usage: powershell -ExecutionPolicy Bypass -File .\start-windows.ps1
# Env (opsional): PORT=8080 PANEL_DATA=./data ADMIN_USER=admin ADMIN_PASS=admin123
#
# Alur dev: edit kode -> jalankan script ini -> buka browser. Tanpa git,
# tanpa install.sh, tanpa systemd. Git + tag + panel-update hanya untuk
# update server produksi.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $MyInvocation.MyCommand.Path)

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Output 'ERROR: Rust belum terinstal.'
    Write-Output 'Install rustup-init.exe dari https://rust-lang.org/tools/install/ lalu buka PowerShell baru.'
    exit 1
}
if (-not $env:PORT) { $env:PORT = '8080' }
if (-not $env:PANEL_DATA) { $env:PANEL_DATA = './data' }
# CARGO_TARGET_DIR default (target/). WSL memakai target-linux (lihat start.sh),
# jadi build Windows dan WSL tidak saling timpa.

if (Get-Command wsl -ErrorAction SilentlyContinue) {
    $wslArgs = @()
    if ($env:PANEL_WSL_DISTRO) { $wslArgs = @('-d', $env:PANEL_WSL_DISTRO) }
    & wsl @wslArgs bash -lc 'exit 0' 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Output 'WSL bridge: OK - terraform/ansible/ssh dijalankan lewat WSL otomatis.'
    } else {
        Write-Output 'WSL bermasalah: distro default mungkin bukan Ubuntu.'
        Write-Output 'Perbaiki sekali saja: wsl --set-default Ubuntu  (atau set $env:PANEL_WSL_DISTRO="Ubuntu")'
    }
} else {
    Write-Output 'WSL tidak ditemukan — mode windows-native.'
    Write-Output 'Butuh terraform.exe di PATH untuk local mode; ansible tidak tersedia di Windows.'
}

Write-Output "Panel dev: http://localhost:$env:PORT  (data: $env:PANEL_DATA\panel.db, login admin/admin123)"
& cargo run
