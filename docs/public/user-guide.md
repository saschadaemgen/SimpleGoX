# SimpleGoX User Guide

## What is SimpleGoX?

SimpleGoX is a multi-messenger desktop client that connects Matrix, Telegram, SimpleX and WhatsApp into a single unified inbox. All protocols appear side by side in one chat list. Your encryption keys and credentials stay on your machine - nothing is routed through cloud bridges.

## Quick Start (Desktop)

### 1. Build and Run

```bash
git clone https://github.com/saschadaemgen/SimpleGoX.git
cd SimpleGoX
npm install
cargo tauri dev
```

### 2. Matrix Login

On first launch you see the login screen. Enter your Matrix credentials:

- **Homeserver:** Your Matrix homeserver URL (e.g. https://matrix.simplego.dev)
- **Username:** Your Matrix username (e.g. sash710)
- **Password:** Your Matrix password

After login, cross-signing keys are automatically created. **Save your Recovery Key** - you need it to verify new devices.

### 3. Connect Telegram (Optional)

1. Open Settings (gear icon in sidebar)
2. Go to the **Accounts** tab
3. Click **+ Add Account** next to Telegram
4. Enter your phone number with country code (e.g. +49 176 xxx xxxx)
5. Enter the code sent to your Telegram app
6. If you have 2FA enabled, enter your password
7. Done - Telegram chats appear in your unified inbox

### 4. Start Messaging

All your Matrix and Telegram chats appear in a single sorted list. Protocol badges (MX, TG) show where each chat lives. Click any chat to read and send messages.

## Settings

Open Settings via the gear icon in the top-right of the sidebar.

### Accounts
Manage your connected messenger accounts. Connect or disconnect Matrix and Telegram. Future protocols (SimpleX, WhatsApp) will appear here when available.

### Appearance
Choose your accent color from 10 presets or enter a custom hex code. Theme selection (currently dark only) will be expanded later.

### Privacy
- **Read Receipts** - Let others know when you read their messages
- **Typing Notices** - Show when you are typing

### Notifications
- **Desktop Notifications** - Show system notifications for new messages
- **Notification Sound** - Play a sound for new messages

### About
Version information, links to GitHub and the project website, license details, and the tech stack.

## Multi-Messenger

### How it Works

Matrix runs directly inside the app. Telegram runs as a separate background process (sidecar) that starts automatically when you launch SimpleGoX.

Each protocol is completely isolated:
- Separate OS processes
- Separate credential storage
- No shared memory between protocols
- A crash in one protocol does not affect others

### Protocol Badges

Every chat in the sidebar shows a small badge:
- **MX** (teal) - Matrix chat
- **TG** (blue) - Telegram chat
- **E2EE** - End-to-end encrypted (Matrix)

### Unified Inbox

All chats from all connected protocols appear in one list, sorted by last activity. You do not need to switch between apps or tabs.

## Telegram

### Authentication

SimpleGoX connects to Telegram using TDLib, the official Telegram Database Library. Your Telegram session is stored locally in the `tdlib-data/` directory. After the first login, the session persists across app restarts - no need to re-enter your phone number.

### Supported Features

- Private chats (1:1 messages)
- Group chats and supergroups
- Channels (read only)
- Text messages (send and receive)
- Stickers and animated emoji (displayed as text emoji)
- Real-time message updates

### Logging Out

1. Open Settings -> Accounts
2. Click **Sign Out** next to your Telegram account
3. The session is destroyed and Telegram chats disappear
4. You can add a new account at any time via **+ Add Account**

## Encryption

All Matrix encryption is handled by [vodozemac](https://github.com/matrix-org/vodozemac), the same library used by Element X, audited by Least Authority. SimpleGoX uses it natively through matrix-rust-sdk - not compiled to WASM.

| Feature | Status |
|:--------|:-------|
| Olm/Megolm (E2E encryption) | Working |
| Cross-signing (MSC4153) | Working |
| Key backup with recovery key | Working |
| Device verification (SAS) | Working |

Telegram uses TDLib's built-in MTProto encryption for client-server communication. End-to-end encrypted secret chats are planned for a future release.

## Troubleshooting

### "Token is not active" (Matrix)
Your session has expired or the device was removed. Delete local data and login again:

**Windows:**
```powershell
Remove-Item -Recurse "$env:LOCALAPPDATA\simplego-x"
Remove-Item -Recurse "$env:APPDATA\simplego-x"
```

**Linux:**
```bash
rm -rf ~/.local/share/simplego-x/
rm -f ~/.config/simplego-x/config.toml
```

### "Can't find the room key" (Matrix)
Messages sent before your device joined cannot be decrypted. This is expected behavior - it means E2E encryption is working correctly. New messages will be readable.

### Telegram shows "Not connected"
The Telegram sidecar may not be running. Open Settings -> Accounts and click **+ Add Account** to restart the connection. If you were previously logged in, the session should auto-restore.

### Empty page after build (Release mode)
The Tauri v2 runtime requires Microsoft Edge WebView2 Runtime. On older Windows 10 systems it may not be installed. Download the latest WebView2 Runtime from Microsoft directly.

### Build fails with "recursion limit"
This is a known upstream issue in matrix-rust-sdk with newer Rust compiler versions. A pull request to fix it exists on the matrix-rust-sdk repository.

## Homeserver

SimpleGoX works with any Matrix homeserver (Synapse, Dendrite, Tuwunel, Conduit). The project maintains a public instance at `matrix.simplego.dev` running Tuwunel 1.5.1 for development and testing.

## Links

- **Website:** [simplego.dev](https://simplego.dev)
- **GitHub:** [github.com/saschadaemgen/SimpleGoX](https://github.com/saschadaemgen/SimpleGoX)
- **License:** Apache-2.0
