# SimpleGoX - Claude Code Instructions

## Project

SimpleGoX is the world's first dedicated Matrix communication terminal and IoT platform.
Built on the Matrix protocol using matrix-rust-sdk.

- Protocol: Matrix Client-Server API v1.18
- Client SDK: matrix-rust-sdk 0.16 (Apache-2.0)
- Cryptography: vodozemac (Olm/Megolm, audited by Least Authority)
- Homeserver: Tuwunel 1.5.1 (Rust, Apache-2.0) - deployed at matrix.simplego.dev
- Repository: github.com/nicokimmel/SimpleGoX
- License: Apache-2.0
- Author: Sascha Daemgen, IT and More Systems, Recklinghausen

## Design Philosophy

SimpleGoX aims to be MORE secure and MORE modern than Element X. Study Element X to
understand matrix-rust-sdk patterns, then build something better. SimpleGoX is not a
clone - it is a purpose-built terminal that leverages dedicated hardware to outperform
any software-only client.

Key principles:
- ALWAYS use the latest, non-deprecated APIs from matrix-rust-sdk
- NEVER use deprecated methods, legacy APIs, or outdated protocol versions
- NEVER compromise on encryption or protocol compliance
- Security architecture: Crypto runs NATIVELY in Rust, NEVER in the WebView
- Reference implementation: https://github.com/element-hq/element-x-android

## Products

1. **SimpleGoX Desktop** (src-tauri/) - Tauri desktop client (Windows + Linux)
2. **SimpleGoX Terminal** (sgx-terminal) - CLI client and future hardware terminal
3. **SimpleGoX Chat** (widget/) - Embeddable E2E-encrypted website chat widget (later)
4. **SimpleGoX IoT** (sgx-iot) - ESP32 Matrix IoT gadget toolkit (later)

## Development Environment

**IMPORTANT: Tauri development happens on Windows (PowerShell), NOT in WSL.**

- IDE: VS Code
- Rust: 1.94.1 (Windows native via rustup)
- Node.js: 22.x (Windows native)
- Tauri CLI: 2.10.1
- Build command: `cargo tauri dev` (in PowerShell)
- WSL is still used for: server deployment, Linux-specific testing, cross-compilation

## Workspace Structure

```
C:\Projects\SimpleGoX
├── Cargo.toml                  # Workspace manifest
├── package.json                # Node.js/Vite for frontend
├── vite.config.js              # Vite dev server config (port 1420)
├── src/                        # Tauri Web Frontend (HTML/CSS/JS)
│   ├── index.html
│   ├── styles/
│   └── js/
├── src-tauri/                  # Tauri Rust Backend
│   ├── Cargo.toml              # Depends on sgx-core
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       └── commands.rs         # Tauri IPC commands
├── crates/
│   ├── sgx-core/               # Shared Matrix client logic (lib)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs       # SgxClient (matrix-sdk wrapper)
│   │       ├── config.rs       # TOML config + session persistence
│   │       └── error.rs        # Error types (thiserror)
│   ├── sgx-terminal/           # CLI application (bin)
│   │   └── src/main.rs
│   └── sgx-iot/                # IoT companion tools (bin)
│       └── src/main.rs
├── widget/                     # Chat widget (TypeScript, later)
├── docs/
│   ├── public/                 # Goes into git (user guide, architecture, compatibility)
│   └── internal/               # GITIGNORED (strategy, business, seasons, hardware)
└── scripts/
    └── setup-dev.sh            # WSL setup script
```

## Rules (NON-NEGOTIABLE)

### Git
- Conventional Commits ONLY: `feat(scope): description`, `fix(scope): description`
- Valid types: feat, fix, docs, test, refactor, ci, chore
- Valid scopes: core, terminal, iot, widget, tauri, ui, docs, ci
- NEVER push to remote - all work stays local until explicitly told otherwise
- NEVER change version numbers in Cargo.toml without explicit permission

### Code Style
- NEVER use em dashes (---) - use regular hyphens (-) or rewrite the sentence
- All code, comments, commits, and documentation in English
- Use `tracing` crate for logging - NEVER `println!` for production output (only for CLI user-facing messages)
- Handle all errors explicitly with `anyhow` or `thiserror` - NEVER use `.unwrap()` in library code
- Run `cargo fmt` before every commit
- Run `cargo clippy` and fix all warnings before every commit
- Write doc comments (`///`) for all public items
- NEVER use placeholder or demo data - ask if values are unknown

### Architecture
- sgx-core is the shared foundation - ALL products depend on it
- src-tauri imports sgx-core for Matrix functionality
- matrix-sdk is ONLY accessed through sgx-core, never from tauri/terminal/iot
- Tauri Commands are the ONLY interface between frontend and backend
- No matrix-sdk imports in the frontend - everything goes through Tauri Commands
- No access tokens, keys, or crypto material in the frontend EVER
- Events for push data (new messages) from backend to frontend via emit()

### Security (Non-Negotiable)
- Crypto runs NATIVELY in Rust, outside the WebView process
- NEVER use libolm - it is deprecated and has unfixed CVEs
- ALWAYS use vodozemac (via matrix-sdk-crypto) for all cryptography
- Cross-signing bootstrapped on first login (MSC4153 mandatory since April 2026)
- Access tokens are secrets - add TODO comments for keychain integration
- NEVER log access tokens, passwords, or encryption keys

### Session Management (Element X Pattern)
- matrix-rust-sdk does NOT store access_token in SQLite - this is by design
- The application MUST persist MatrixSession separately (access_token, device_id, user_id, refresh_token)
- On restart, call `client.restore_session(session)` with persisted credentials
- Use `subscribe_to_session_changes()` to detect and re-persist token refreshes

### Data Locations
- Config: `~/.config/simplego-x/config.toml` (or %APPDATA% on Windows)
- Data: `~/.local/share/simplego-x/` (or %LOCALAPPDATA% on Windows)

## Build and Test

```powershell
# Tauri Desktop Client (Windows PowerShell)
cd C:\Projects\SimpleGoX
cargo tauri dev

# CLI only (WSL or PowerShell)
cargo build
cargo run -p sgx-terminal -- --help
cargo test
cargo clippy
cargo fmt --check
```

## Known Issues

- matrix-sdk 0.16 + Rust 1.94+ causes recursion depth overflow
- Workaround: `#![recursion_limit = "256"]` in lib.rs and main.rs files
- WSL workaround: `sed -i '1i #![recursion_limit = "256"]' ~/.cargo/registry/src/index.crates.io-*/matrix-sdk-0.16.0/src/lib.rs`
- Recovery/backup setup fails on repeated logins ("backup already exists")
- Cross-signing status shows "no" on session restore

## Current State (Season 1 - Day 1 Complete)

### Working
- [x] Cargo workspace with 4 targets: sgx-core, sgx-terminal, sgx-iot, src-tauri
- [x] matrix-rust-sdk 0.16 compiling on Rust 1.94.1
- [x] Login to matrix.org and own homeserver (simplego.dev)
- [x] Hidden password input, config persistence, session restore
- [x] Cross-signing bootstrap, recovery key generation
- [x] E2E encrypted messages sent AND received
- [x] Send, verify, logout commands
- [x] Auto-join on room invitations
- [x] Tuwunel 1.5.1 deployed, federation working
- [x] Tauri desktop window renders

### In Progress
- [ ] Tauri UI: Login screen, room list, chat view, message sending

## Key Resources

- matrix-rust-sdk docs: https://docs.rs/matrix-sdk/0.16.0
- Tauri v2 docs: https://v2.tauri.app
- Element X Android: https://github.com/element-hq/element-x-android
- Own homeserver: https://matrix.simplego.dev
