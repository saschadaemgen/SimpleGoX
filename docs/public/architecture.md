# SimpleGoX Architecture

## Overview

SimpleGoX is a Cargo workspace with four crates sharing a common foundation.

```
src-tauri (Desktop) ──┐
sgx-terminal (CLI) ───┤
                      ├──> sgx-core ──> matrix-sdk ──> vodozemac (E2EE)
sgx-iot (IoT) ────────┘                     │
                                             └──> matrix-sdk-sqlite (storage)
```

## Security Model

SimpleGoX separates the UI from all security-critical operations:

**Frontend (WebView)** displays the UI only. It has no access to encryption keys, access tokens, or matrix-sdk. Communication with the backend happens exclusively through Tauri Commands (IPC), which pass serialized data but never key material.

**Rust Backend** runs sgx-core with matrix-sdk and vodozemac natively. Crypto runs in a separate process that the WebView cannot access. Keys never leave this process.

This is fundamentally different from Element Desktop where vodozemac runs as WASM inside the Chromium process. In SimpleGoX, the WebView is untrusted by design.

## Crates

### sgx-core (library)

The shared Matrix client logic. All products build on this crate.

- `client.rs` - High-level client wrapper (SgxClient) around matrix-sdk
- `config.rs` - TOML-based configuration with session persistence
- `error.rs` - Unified error types (thiserror)

Handles: authentication, session restore, E2E encryption lifecycle, cross-signing bootstrap, sync loop with callbacks, message sending/receiving, typing indicators, read receipts, room summaries.

### src-tauri (Tauri desktop app)

The desktop client using Tauri v2. Rust backend with web frontend.

- `lib.rs` - Tauri app entry point, state management
- `commands.rs` - IPC command handlers (login, get_rooms, send_message, send_typing, mark_as_read, get_settings, logout)

The frontend (src/) is HTML/CSS/JS using Tauri's IPC to communicate with the backend. No matrix-sdk types cross the IPC boundary.

### sgx-terminal (binary)

CLI client with login, run, send, verify, and logout subcommands. Uses clap for argument parsing, rpassword for secure password input.

### sgx-iot (binary)

Host-side companion tools for ESP32 Matrix IoT gadgets. The actual ESP32 firmware is written in C (see SimpleGoX-ESP).

## Key Design Decisions

- **matrix-sdk accessed only through sgx-core** - all other crates never import matrix-sdk directly
- **Tauri Commands are the only IPC interface** - no tokens, keys, or crypto material in the frontend
- **Session tokens persisted by the application** - matrix-sdk stores crypto state in SQLite, but session credentials (access_token, device_id) are managed by sgx-core's config
- **Cross-signing bootstrapped on first login** - mandatory since MSC4153 (April 2026)
- **vodozemac only** - libolm is deprecated and not used anywhere
- **Sync via clone** - the Sync-Loop runs on a cloned Client to avoid Mutex deadlocks

## Data Storage

### Windows

```
%APPDATA%\simplego-x\
    └── config.toml              # Homeserver URL, username, session tokens

%LOCALAPPDATA%\simplego-x\
    ├── matrix-sdk-state.db      # Room state, sync tokens
    └── matrix-sdk-crypto.db     # Encryption keys, device lists, cross-signing
```

### Linux

```
~/.config/simplego-x/
    └── config.toml

~/.local/share/simplego-x/
    ├── matrix-sdk-state.db
    └── matrix-sdk-crypto.db
```

## Building

### Desktop Client (Tauri)

```bash
# Development (Windows PowerShell or Linux terminal)
npm install
cargo tauri dev

# Production build
cargo tauri build
```

### CLI Client

```bash
cargo build
cargo run -p sgx-terminal -- --help
```

### Full workspace

```bash
cargo build              # Debug build
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt --check        # Format check
```

## Infrastructure

| Component | Details |
|:----------|:--------|
| Homeserver | Tuwunel 1.5.1 at matrix.simplego.dev |
| Deployment | Docker on Debian 13 VPS |
| Reverse Proxy | nginx with stream proxy (TLS termination) |
| Federation | Verified with matrix.org via .well-known delegation |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for commit conventions and code style.
