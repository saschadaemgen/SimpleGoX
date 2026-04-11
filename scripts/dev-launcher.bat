@echo off
REM ================================================
REM SimpleGoX Development Launcher (Windows)
REM Community contribution by Gas Lighter
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
echo.
echo ======================================
echo   Starting SimpleGoX Dev Environment
echo ======================================
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
