@echo off
REM ================================================
REM SimpleGoX Development Launcher (Windows)
REM Community contribution by Pablo
REM
REM Starts the Telegram sidecar and Tauri dev app
REM in separate windows, then arranges them on
REM screen for comfortable debugging.
REM
REM NOTE: The Tauri app auto-starts the sidecar
REM if a Telegram session exists. This script is
REM useful for fresh installs or when you want
REM both console windows visible for debugging.
REM
REM Get your API credentials at my.telegram.org
REM ================================================

title SimpleGoX Dev Launcher
set "PROJECT_PATH=YOUR_PROJECT_PATH"
set "API_ID=YOUR_APP_ID"
set "API_HASH=YOUR_APP_HASH"

REM --- Check if cargo tauri dev is already running ---
powershell -NoProfile -Command "$p = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'cargo.exe' -and $_.CommandLine -match '\btauri\s+dev\b' }; if ($p) { exit 1 } else { exit 0 }"
if %errorlevel% neq 0 (
    echo.
    echo Detected "cargo tauri dev" is already running.
    echo Close it first, then restart this script to continue.
    echo.
    pause
    exit /b 1
)

REM --- Kill stale sidecar from previous session ---
taskkill /IM sgx-telegram.exe /F >nul 2>&1

REM --- Rebuild Telegram sidecar (ensures proto is always up-to-date) ---
echo.
echo ======================================
echo   Starting SimpleGoX Dev Environment
echo ======================================
echo.
echo [0/2] Rebuilding Telegram sidecar (proto sync)...
cargo build -p sgx-telegram
if errorlevel 1 (
    echo.
    echo ERROR: Telegram sidecar build failed!
    echo Fix the errors above and try again.
    echo.
    pause
    exit /b 1
)
echo      Sidecar build OK.
echo.

echo [1/2] Starting Telegram sidecar
start "SimpleGoX - Telegram Sidecar" /d "%PROJECT_PATH%" cmd /k "title SimpleGoX - Telegram Sidecar && echo Telegram sidecar is running && cargo run -p sgx-telegram -- --api-id %API_ID% --api-hash %API_HASH% --port 50051"
echo Waiting 2 seconds for sidecar to initialize
timeout /t 2 /nobreak >nul
echo [2/2] Starting Tauri dev app
start "SimpleGoX - Tauri DevApp" /d "%PROJECT_PATH%" cmd /k "title SimpleGoX - Tauri DevApp && echo Tauri dev app is running && cargo tauri dev"
echo Waiting 4 seconds for windows to appear
timeout /t 4 /nobreak >nul
powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; Add-Type 'using System; using System.Runtime.InteropServices; public class Win32 { [DllImport(\"user32.dll\")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint); }'; Start-Sleep -Milliseconds 800; $cmds = Get-Process cmd | Where-Object { $_.MainWindowHandle -ne 0 } | Sort-Object StartTime -Descending | Select-Object -First 2; if ($cmds.Count -ge 2) { $screen = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea; $halfW = [int]($screen.Width / 2); $h = $screen.Height; [Win32]::MoveWindow($cmds[1].MainWindowHandle, 0, 0, $halfW, $h, $true); [Win32]::MoveWindow($cmds[0].MainWindowHandle, $halfW, 0, $halfW, $h, $true) }; $app = Get-Process | Where-Object { $_.MainWindowTitle -like '*SimpleGoX*' } | Select-Object -First 1; if ($app) { $screen = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea; [Win32]::MoveWindow($app.MainWindowHandle, 100, 100, 1200, 800, $true) }"
