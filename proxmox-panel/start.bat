@echo off
chcp 65001 >nul
title Proxmox Panel (Rust)
cd /d "%~dp0"

echo ========================================
echo  Proxmox Panel - Rust + SB Admin 2
echo ========================================
echo.

cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust not found. Install from https://rustup.rs/
    pause
    exit /b 1
)

wsl bash -lc "echo wsl-ok" >nul 2>&1
if errorlevel 1 (
    echo [WARN] WSL not available. Local exec disabled, remote SSH via WSL also disabled.
) else (
    echo [OK] WSL available.
)

echo [1/2] Building (release)...
cargo build --release
if errorlevel 1 (
    echo [ERROR] build failed
    pause
    exit /b 1
)

echo [2/2] Starting server...
echo     Panel: http://localhost:8080
echo     Health: http://localhost:8080/health.html
echo.
echo     Press Ctrl+C to stop
echo.

set PORT=8080
.\target\release\proxmox-panel.exe
pause
