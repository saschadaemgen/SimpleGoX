<p align="center">
  <h1 align="center">SimpleGoX</h1>
</p>

<p align="center">
  <strong>Open-source multi-messenger desktop client connecting Matrix, Telegram, SimpleX and WhatsApp in one unified inbox.</strong><br>
  Built with Rust, Tauri v2 and Svelte. All cryptography runs natively - encryption keys never touch the WebView.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://matrix.org"><img src="https://img.shields.io/badge/Protocol-Matrix-black.svg" alt="Matrix"></a>
  <a href="https://core.telegram.org"><img src="https://img.shields.io/badge/Protocol-Telegram-blue.svg" alt="Telegram"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Built_with-Rust-orange.svg" alt="Rust"></a>
  <a href="https://v2.tauri.app"><img src="https://img.shields.io/badge/Desktop-Tauri_v2-blue.svg" alt="Tauri"></a>
</p>

---

## What is SimpleGoX?

SimpleGoX is a multi-messenger desktop client that connects Matrix, Telegram, SimpleX and WhatsApp into a single unified inbox. Every protocol runs as an isolated native process - your Telegram keys never leave your machine, your Matrix encryption runs in Rust outside the browser, and each messenger is completely separated from the others.

Unlike Beeper which bridges everything through Matrix on the server side, SimpleGoX connects to each protocol natively on the client side. Zero shared memory between protocols. No server-side bridges. No cloud proxy.

The project consists of three products:

| Product | Description | Status |
|:--------|:------------|:-------|
| **SimpleGoX Desktop** | Multi-messenger Tauri desktop client | In development |
| **[SimpleGoX Chat](https://github.com/saschadaemgen/SimpleGoX-Chat)** | Embeddable E2E-encrypted website chat widget | Planned |
| **[SimpleGoX ESP](https://github.com/saschadaemgen/SimpleGoX-ESP)** | ESP32 IoT devices that speak Matrix | In development |

---

## Protocols

| Protocol | Status | Implementation |
|:---------|:-------|:---------------|
| **Matrix** | Working | Native Rust via matrix-rust-sdk 0.16 + vodozemac |
| **Telegram** | Working | TDLib 1.8.61 via tdlib-rs in isolated sidecar process |
| **SimpleX** | Season 3 | Native Rust SMP implementation (planned) |
| **WhatsApp** | Season 4 | Official EU DMA interoperability path (planned) |

---

## Why SimpleGoX?

**One inbox, four protocols.** Matrix and Telegram chats appear side by side in a single chat list, sorted by last activity. Protocol badges (MX, TG) show where each chat lives. You never need to switch apps.

**Native cryptography.** Element Desktop runs vodozemac as WASM inside Chromium. SimpleGoX runs vodozemac natively in Rust, outside the WebView process. Keys never enter the browser context.

**Process isolation.** Each messenger backend runs as a separate OS process communicating over gRPC. A crash in TDLib cannot take down your Matrix session. Telegram credentials are isolated from Matrix key material.

**No server bridges.** Your Telegram session runs locally via TDLib. Your keys and credentials never leave your machine. This is fundamentally different from server-mediated bridge architectures.

**Small and fast.** Tauri produces a 5-10 MB installer instead of 200+ MB (Electron). Starts in under a second. Uses the system WebView instead of bundling Chromium.

---

## Architecture

```
Svelte 5 Frontend (WebView)
        |
        | Tauri IPC (commands + events)
        |
Tauri v2 Rust Backend
        |
        +-- sgx-core (Matrix via matrix-rust-sdk)
        |
        +-- gRPC Client --> sgx-telegram Sidecar (TDLib)
        |
        +-- gRPC Client --> sgx-simplex Sidecar (planned)
        |
        +-- gRPC Client --> sgx-whatsapp Sidecar (planned)
```

**Frontend (Svelte 5)** - UI only, no access to keys or tokens.

**Tauri Backend (Rust)** - Matrix client runs in-process. External protocols connect via gRPC to isolated sidecar binaries.

**Sidecar Processes** - Each non-Matrix protocol runs as a separate binary. Communicates over localhost gRPC using a shared protobuf schema (messenger.proto). Complete process isolation.

**Protobuf Contract** - All sidecars implement the same `MessengerService` gRPC interface. Adding a new protocol means implementing one more sidecar - the frontend and Tauri backend remain untouched.

---

## Features

### Messaging
- End-to-end encrypted messaging (Olm/Megolm via vodozemac)
- Cross-signing with recovery key
- Federation (tested: simplego.dev <-> matrix.org)
- Telegram message sending and receiving
- Unified chat list across protocols
- Message grouping with stacked bubbles
- Reply, reactions, edit, redact (Matrix)
- Emoji and animated emoji support (Telegram)
- Date separators (Today, Yesterday, full date)

### Multi-Messenger
- Protocol badges (MX, TG) on every chat
- Single sorted inbox across all protocols
- Isolated sidecar architecture per protocol
- gRPC streaming for real-time updates
- Automatic sidecar startup and session restore

### UI/UX
- Custom bubble design with two-part layout
- Circular avatars with quarter-cut effect
- Accent color picker with 10 presets + custom hex
- Settings panel with tabbed fullscreen overlay
- Account management (connect/disconnect protocols)
- Dark theme
- Sender colors (deterministic per user)

### Security
- All cryptography runs natively in Rust (not WASM)
- Encryption keys never enter the WebView process
- Each protocol isolated in separate OS process
- No server-side bridges - all connections are local
- Telegram credentials never leave the device

---

## Building

### Prerequisites

- Rust 1.85+ (via [rustup](https://rustup.rs))
- Node.js 20+ (via [nodejs.org](https://nodejs.org))
- Tauri CLI: `cargo install tauri-cli --version "^2"`

**Windows:** Visual Studio Build Tools (installed automatically by rustup)

**Linux:** `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`

### Development

```bash
cd SimpleGoX
npm install
cargo tauri dev
```

### Telegram sidecar (separate terminal)

```bash
cargo build -p sgx-telegram
.\target\debug\sgx-telegram.exe --api-id YOUR_API_ID --api-hash YOUR_API_HASH --port 50051
```

### Production build

```bash
cargo tauri build
```

Output: `.msi` (Windows) or `.deb`/`.AppImage` (Linux) in `target/release/bundle/`

---

## Project structure

```
SimpleGoX/
├── crates/
│   ├── sgx-core/               # Matrix client (matrix-rust-sdk wrapper)
│   ├── sgx-proto/              # Shared protobuf/gRPC definitions
│   └── sgx-telegram/           # Telegram sidecar (TDLib + gRPC server)
├── proto/
│   └── messenger.proto         # Unified messenger service contract
├── src-tauri/                  # Tauri v2 Rust backend
│   └── src/
│       ├── lib.rs              # App entry, sidecar management
│       ├── commands.rs         # Matrix IPC commands
│       ├── telegram_commands.rs # Telegram IPC commands
│       └── sidecar.rs          # Sidecar process manager
├── src/                        # Svelte 5 frontend
│   ├── components/
│   │   ├── ChatView.svelte     # Message display
│   │   ├── RoomList.svelte     # Unified chat sidebar
│   │   ├── Settings.svelte     # Fullscreen settings overlay
│   │   └── settings/           # Settings tab components
│   ├── lib/
│   │   ├── stores.js           # Svelte stores
│   │   └── tauri.js            # Tauri command wrappers
│   └── App.svelte
└── docs/
```

---

## SimpleGoX ecosystem

| Project | Description | Repository |
|:--------|:------------|:-----------|
| **SimpleGoX** | Multi-messenger desktop client | [GitHub](https://github.com/saschadaemgen/SimpleGoX) |
| **SimpleGoX Chat** | E2E-encrypted website chat widget | [GitHub](https://github.com/saschadaemgen/SimpleGoX-Chat) |
| **SimpleGoX ESP** | ESP32 Matrix IoT devices | [GitHub](https://github.com/saschadaemgen/SimpleGoX-ESP) |

SimpleGoX is the successor to [SimpleGo](https://github.com/saschadaemgen/SimpleGo) (ESP32 hardware messenger) and [GoChat](https://github.com/saschadaemgen/GoChat) (browser chat widget), now built on the Matrix protocol with multi-messenger support.

---

## Homeserver

SimpleGoX includes deployment support for [Tuwunel](https://github.com/matrix-construct/tuwunel), a lightweight Rust-based Matrix homeserver. The project maintains a public instance at `matrix.simplego.dev` for development and testing.

SimpleGoX works with any Matrix homeserver (Synapse, Dendrite, Tuwunel, Conduit).

---

## Roadmap

| Season | Focus | Status |
|:-------|:------|:-------|
| Season 1 | Foundation - Matrix client, homeserver, federation, desktop GUI | Complete |
| Season 2 | Multi-messenger - Svelte 5 migration, Telegram integration, UI polish | In progress |
| Season 3 | SimpleX protocol - native Rust SMP implementation | Planned |
| Season 4 | WhatsApp - official EU DMA interoperability | Planned |
| Season 5 | Hardware - Raspberry Pi image, dedicated terminal | Future |
| Season 6+ | Custom hardware with secure elements | Future |

---

## License

Apache-2.0

## Acknowledgments

[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) (Matrix client SDK) - [vodozemac](https://github.com/matrix-org/vodozemac) (Olm/Megolm cryptography) - [Tauri](https://v2.tauri.app) (desktop framework) - [tdlib-rs](https://github.com/FedericoBruzzone/tdlib-rs) (Telegram Database Library wrapper) - [Tuwunel](https://github.com/matrix-construct/tuwunel) (homeserver)

---

<p align="center">
  <i>SimpleGoX is an independent open-source project by <a href="https://it-and-more.systems">IT and More Systems</a>, Recklinghausen, Germany.</i>
</p>

<p align="center">
  <a href="https://simplego.dev">simplego.dev</a>
</p>
