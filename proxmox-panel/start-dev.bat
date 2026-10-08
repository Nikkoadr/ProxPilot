@echo off
chcp 65001 >nul
title Proxmox Panel - Dev Mode
cd /d "%~dp0"

echo ========================================
echo  Proxmox Panel - Development Mode
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

echo [1/4] Installing Go modules...
cd backend
go mod tidy
cd ..

echo [2/4] Installing frontend dependencies...
cd frontend
if not exist "node_modules" (
    call npm install
)
cd ..

echo [3/4] Starting backend on :8080...
start "Proxmox Panel - Backend" cmd /c "cd backend && go run main.go & pause"

echo [4/4] Starting frontend dev server on :5173...
start "Proxmox Panel - Frontend" cmd /c "cd frontend && npm run dev & pause"

echo.
echo ========================================
echo  Backend : http://localhost:8080
echo  Frontend: http://localhost:5173
echo ========================================
timeout /t 3 >nul
