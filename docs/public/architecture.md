# SimpleGoX Architecture

## Overview

SimpleGoX is a multi-messenger desktop client built as a Cargo workspace. Matrix runs in-process via matrix-rust-sdk. External protocols (Telegram, SimpleX, WhatsApp) run as isolated sidecar processes communicating over gRPC.

```
Svelte 5 Frontend (WebView)
        |
        | Tauri IPC (commands + events)
        |
src-tauri (Tauri v2 Rust Backend)
        |
        +-- sgx-core ---------> matrix-rust-sdk --> vodozemac (E2EE)
        |                              |
        |                              +----------> matrix-sdk-sqlite (storage)
        |
        +-- gRPC Client ------> sgx-telegram (Sidecar Process)
        |                              |
        |                              +----------> TDLib 1.8.61 (tdjson.dll)
        |
        +-- gRPC Client ------> sgx-simplex (planned)
        |
        +-- gRPC Client ------> sgx-whatsapp (planned)
```

## Security Model

SimpleGoX separates the UI from all security-critical operations using two isolation layers:

**Layer 1 - WebView Isolation:** The frontend (Svelte 5) displays the UI only. It has no access to encryption keys, access tokens, or matrix-sdk. Communication with the backend happens exclusively through Tauri Commands (IPC), which pass serialized data but never key material.

**Layer 2 - Process Isolation:** Each external protocol runs as a separate OS process. Telegram credentials are in a different process than Matrix keys. A crash in TDLib cannot affect the Matrix session. Processes communicate over localhost gRPC with typed protobuf messages.

This is fundamentally different from Element Desktop where vodozemac runs as WASM inside the Chromium process, and from Beeper which bridges everything through server-side Matrix bridges.

## Crates

### sgx-core (library)

The shared Matrix client logic. Handles authentication, session restore, E2E encryption lifecycle, cross-signing bootstrap, sync loop with callbacks, message sending/receiving, typing indicators, read receipts, room summaries.

Key files:
- `client.rs` - High-level client wrapper (SgxClient) around matrix-sdk
- `config.rs` - TOML-based configuration with session persistence
- `error.rs` - Unified error types (thiserror)

### sgx-proto (library)

Shared protobuf/gRPC definitions generated from `proto/messenger.proto`. Defines the `MessengerService` interface that all sidecar processes implement.

Key types: UnifiedMessage, Chat, ChatId, UserId, Update, AuthState

### sgx-telegram (binary)

Telegram sidecar process. Runs TDLib 1.8.61 via tdlib-rs and exposes a gRPC server implementing MessengerService.

Key files:
- `main.rs` - gRPC server entry point with CLI argument parsing
- `service.rs` - MessengerService gRPC implementation
- `convert.rs` - TDLib types to protobuf type mapping
- `auth.rs` - TDLib authentication state machine

Architecture: A pump thread calls `tdlib_rs::receive()` in a loop on a dedicated OS thread. Responses are matched to requests via `@extra` tags and delivered through an Observer pattern. Auth state changes are broadcast to all subscribers.

### src-tauri (Tauri desktop app)

The desktop client using Tauri v2 with Svelte 5 frontend.

Key files:
- `lib.rs` - App entry point, sidecar auto-start, state management
- `commands.rs` - Matrix IPC command handlers
- `telegram_commands.rs` - Telegram IPC command handlers (gRPC forwarding)
- `sidecar.rs` - SidecarManager for spawning and connecting to protocol processes

## Protobuf Service Contract

All sidecar processes implement the same gRPC interface defined in `proto/messenger.proto`:

```protobuf
service MessengerService {
  // Auth
  rpc GetAuthState(GetAuthStateRequest) returns (AuthState);
  rpc SubmitPhoneNumber(SubmitPhoneNumberRequest) returns (AuthState);
  rpc SubmitAuthCode(SubmitAuthCodeRequest) returns (AuthState);
  rpc SubmitPassword(SubmitPasswordRequest) returns (AuthState);
  rpc Logout(LogoutRequest) returns (LogoutResponse);

  // Chats & Messages
  rpc ListChats(ListChatsRequest) returns (ListChatsResponse);
  rpc SendMessage(SendMessageRequest) returns (UnifiedMessage);
  rpc GetChatHistory(GetChatHistoryRequest) returns (GetChatHistoryResponse);

  // Real-time
  rpc StreamUpdates(StreamUpdatesRequest) returns (stream Update);

  // ... additional RPCs for media, contacts, reactions
}
```

Adding a new protocol means implementing one more sidecar binary against this contract. The Tauri backend and Svelte frontend remain untouched.

## Frontend Architecture

The Svelte 5 frontend uses reactive stores and a component-based architecture:

```
App.svelte
  +-- ChatLayout.svelte
        +-- Sidebar.svelte
        |     +-- RoomList.svelte
        |           +-- RoomItem.svelte (per chat)
        |                 +-- Avatar.svelte
        +-- ChatView.svelte
        |     +-- MessageBubble (per message)
        +-- Settings.svelte (fullscreen overlay)
              +-- AccountsTab.svelte
              +-- AppearanceTab.svelte
              +-- PrivacyTab.svelte
              +-- NotificationsTab.svelte
              +-- AboutTab.svelte
```

Protocol-agnostic design: The frontend does not know which protocol a message comes from. It renders UnifiedMessage objects identically regardless of source. Protocol badges (MX, TG) are derived from the ChatId backend field.

## Key Design Decisions

- **Matrix in-process, everything else out-of-process** - Matrix is the core protocol and runs natively in the Tauri backend. External protocols run as sidecars for isolation.
- **gRPC over localhost TCP** - Simple, cross-platform, typed. No Named Pipes complexity.
- **Single protobuf contract** - All protocols implement the same interface. The frontend is protocol-agnostic.
- **TDLib pagination loop** - TDLib's getChatHistory returns fewer messages than requested by design. A pagination loop collects all messages across multiple calls.
- **Broadcast channel for updates** - TDLib events flow through a tokio broadcast channel to support multiple StreamUpdates subscribers.
- **Tauri Event System for real-time** - gRPC stream updates are forwarded as Tauri events to the Svelte frontend for instant UI updates.
- **Session persistence** - Both Matrix (config.toml + SQLite) and Telegram (tdlib-data/) sessions survive app restarts. No re-login needed.

## Data Storage

### Windows

```
%APPDATA%\simplego-x\
    +-- config.toml              # Homeserver URL, username, session tokens

%LOCALAPPDATA%\simplego-x\
    +-- matrix-sdk-state.db      # Room state, sync tokens
    +-- matrix-sdk-crypto.db     # Encryption keys, device lists, cross-signing

tdlib-data\                      # Telegram session (relative to binary)
    +-- td.binlog                # TDLib session data
    +-- db/                      # TDLib message database
```

### Linux

```
~/.config/simplego-x/
    +-- config.toml

~/.local/share/simplego-x/
    +-- matrix-sdk-state.db
    +-- matrix-sdk-crypto.db

tdlib-data/
    +-- td.binlog
    +-- db/
```

## Building

### Desktop Client (Tauri + Svelte)

```bash
npm install
cargo tauri dev          # Development
cargo tauri build        # Production
```

### Telegram Sidecar

```bash
cargo build -p sgx-telegram
.\target\debug\sgx-telegram.exe --api-id YOUR_API_ID --api-hash YOUR_API_HASH --port 50051
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
| Deployment | Docker on Debian 13 VPS (194.164.197.247) |
| Reverse Proxy | nginx with stream proxy (ssl_preread, port 443) |
| Federation | Verified with matrix.org via .well-known delegation |
| Telegram API | api_id registered at my.telegram.org |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for commit conventions and code style.
