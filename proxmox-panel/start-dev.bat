@echo off
chcp 65001 >nul
title Proxmox Panel - Dev (Rust)
cd /d "%~dp0"

cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust not found. Install from https://rustup.rs/
    pause
    exit /b 1
)

echo Dev server: http://localhost:8080 (static/ served directly, no build step)
echo Press Ctrl+C to stop
set PORT=8080
cargo run
pause
