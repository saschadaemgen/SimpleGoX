@echo off
REM ================================================
REM SimpleGoX Quick Dev Launcher
REM
REM Kills stale sidecar, rebuilds it (ensures proto
REM is always up-to-date), then starts cargo tauri dev.
REM ================================================

echo === SimpleGoX Dev ===

REM Kill stale sidecar
taskkill /IM sgx-telegram.exe /F >nul 2>&1

REM Rebuild sidecar
echo Building Telegram sidecar...
cargo build -p sgx-telegram
if errorlevel 1 (
    echo.
    echo ERROR: Sidecar build failed!
    pause
    exit /b 1
)
echo Sidecar build OK.
echo.

REM Start Tauri dev
echo Starting SimpleGoX...
cargo tauri dev
