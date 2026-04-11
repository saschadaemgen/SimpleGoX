# SimpleGoX Scripts

Development helper scripts for SimpleGoX.

## dev-launcher.bat (Windows)

Starts the Telegram sidecar and Tauri dev app in separate console
windows, arranged side by side for debugging.

*Community contribution by Gas Lighter*

### Important note

Since v0.0.1-pre-alpha, the Tauri app auto-starts the Telegram
sidecar when a saved session exists. For normal development you
only need:

```
cargo tauri dev
```

This script is useful when you need to **debug the sidecar separately**,
for example when troubleshooting TDLib authentication, gRPC connection
issues, or sidecar crashes. It gives you a dedicated console window
for the sidecar logs.

**Do not run this script while `cargo tauri dev` is already running.**
The auto-started sidecar locks the binary file and the script will
fail with "Zugriff verweigert" (access denied).

### Setup

1. Copy `dev-launcher.bat` anywhere on your PC
2. Edit the file and set your values:

```batch
set "PROJECT_PATH=C:\Projects\SimpleGoX"
set "API_ID=your_telegram_api_id"
set "API_HASH=your_telegram_api_hash"
```

3. Make sure no SimpleGoX instance is running
4. Double-click to start

### Getting Telegram API credentials

1. Go to [my.telegram.org](https://my.telegram.org)
2. Log in with your phone number
3. Go to "API development tools"
4. Create an application
5. Copy the `api_id` and `api_hash`

### What it does

1. Starts `cargo run -p sgx-telegram` in its own console window
2. Waits 2 seconds for the sidecar to initialize
3. Starts `cargo tauri dev` in a second console window
4. Arranges windows: sidecar console left, Tauri console right
5. Positions the SimpleGoX app window in the center
