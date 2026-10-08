@echo off
chcp 65001 >nul
title Proxmox Panel
cd /d "%~dp0"

echo ========================================
echo  Proxmox Panel - Kubernetes Deployer
echo ========================================
echo.

REM Check Go
go version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Go not found. Install Go 1.22+ from https://go.dev/dl/
    pause
    exit /b 1
)

REM Check Node.js
node --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Node.js not found. Install Node.js 18+ from https://nodejs.org/
    pause
    exit /b 1
)

echo [1/4] Checking Go modules...
cd backend
go mod tidy >nul 2>&1
if errorlevel 1 (
    echo [WARN] go mod tidy failed, continuing...
)
cd ..

echo [2/4] Installing frontend dependencies...
cd frontend
if not exist "node_modules" (
    call npm install
) else (
    echo     node_modules exists, skipping npm install
)
cd ..

echo [3/4] Building frontend...
cd frontend
call npm run build
cd ..

echo [4/4] Starting backend server...
echo     Backend: http://localhost:8080
echo     Frontend: http://localhost:8080 (served by backend)
echo.
echo     Press Ctrl+C to stop
echo.

cd backend
start "Proxmox Panel" cmd /c "go run main.go & pause"

echo.
echo ========================================
echo  Panel started! Open http://localhost:8080
echo ========================================
timeout /t 3 >nul
