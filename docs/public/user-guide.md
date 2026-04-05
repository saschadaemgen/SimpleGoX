# SimpleGoX User Guide

## What is SimpleGoX?

SimpleGoX is a dedicated Matrix communication terminal. It connects to any Matrix homeserver and provides end-to-end encrypted messaging on purpose-built hardware.

## Quick Start

### 1. Install

```bash
# Clone the repository
git clone https://github.com/nicokimmel/SimpleGoX.git
cd SimpleGoX

# Build
cargo build --release

# Run
./target/release/sgx-terminal --help
```

### 2. Login

```bash
sgx-terminal login --homeserver https://matrix.org --user your_username
```

You will be prompted for your password (hidden input). After login, cross-signing keys are automatically created for secure communication.

**Save your Recovery Key!** It will be displayed after login. Store it in a safe place - you need it to verify new devices.

### 3. Start Messaging

```bash
sgx-terminal run
```

The terminal connects to your homeserver and displays incoming messages in real-time. Press Ctrl+C to stop.

### 4. Configuration

Your config is stored at `~/.config/simplego-x/config.toml`:

```toml
homeserver_url = "https://matrix.org"
username = "your_username"
encryption = true
```

Data (encryption keys, message cache) is stored at `~/.local/share/simplego-x/`.

## Supported Homeservers

SimpleGoX works with **any** Matrix homeserver that supports the Client-Server API:

| Homeserver | Status |
|---|---|
| matrix.org | Fully supported |
| Self-hosted Synapse | Fully supported |
| Self-hosted Tuwunel | Fully supported |
| Element Server Suite | Fully supported |
| Any Matrix CS API v1.11+ server | Fully supported |

Protocol compatibility is automatic. If your homeserver speaks Matrix, SimpleGoX speaks to it.

## Troubleshooting

### "Token is not active"
Your session has expired or the device was removed. Delete local data and login again:
```bash
rm -rf ~/.local/share/simplego-x/
rm -f ~/.config/simplego-x/config.toml
sgx-terminal login --homeserver https://matrix.org --user your_username
```

### "Can't find the room key"
Messages sent before your device joined cannot be decrypted. This is expected behavior - it means E2E encryption is working correctly. New messages will be readable.

### Build fails with "recursion limit"
Run this once:
```bash
sed -i '1i #![recursion_limit = "256"]' ~/.cargo/registry/src/index.crates.io-*/matrix-sdk-0.16.0/src/lib.rs
```

## Security

- All messages are end-to-end encrypted using the Olm/Megolm protocol (via vodozemac)
- Cross-signing is automatically configured on first login
- Encryption keys are stored locally in SQLite
- Your password is never stored - only the session token
- The same cryptographic stack used by Element X
