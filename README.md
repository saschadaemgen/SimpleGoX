<p align="center">
  <h1 align="center">SimpleGoX</h1>
</p>

<p align="center">
  <strong>Secure Matrix communication terminal with native Rust cryptography.</strong><br>
  End-to-end encrypted. Federated. Built for dedicated hardware.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://matrix.org"><img src="https://img.shields.io/badge/Protocol-Matrix-black.svg" alt="Matrix"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Built_with-Rust-orange.svg" alt="Rust"></a>
  <a href="https://v2.tauri.app"><img src="https://img.shields.io/badge/Desktop-Tauri_v2-blue.svg" alt="Tauri"></a>
</p>

---

## What is SimpleGoX?

SimpleGoX is a Matrix client built from the ground up for security and dedicated hardware. Unlike browser-based clients, SimpleGoX runs all cryptography natively in Rust - encryption keys never touch the WebView.

The project consists of three products sharing a common Rust core:

| Product | Description | Status |
|:--------|:------------|:-------|
| **SimpleGoX Desktop** | Tauri desktop client for Windows and Linux | In development |
| **[SimpleGoX Chat](https://github.com/saschadaemgen/SimpleGoX-Chat)** | Embeddable E2E-encrypted website chat widget | Planned |
| **[SimpleGoX ESP](https://github.com/saschadaemgen/SimpleGoX-ESP)** | ESP32 IoT devices that speak Matrix | Planned |

---

## Why another Matrix client?

**Native cryptography.** Element Desktop runs vodozemac as WASM inside Chromium. SimpleGoX runs vodozemac natively in Rust, outside the WebView process. Keys never enter the browser context. This is a measurable reduction in attack surface.

**Small and fast.** Tauri produces a 5-10 MB installer instead of 200+ MB (Electron). Starts in under a second. Uses the system WebView instead of bundling Chromium.

**Hardware-ready.** The same codebase runs on Windows, Linux desktop, and Raspberry Pi. The long-term goal is a dedicated hardware terminal with secure elements and tamper detection.

---

## Architecture

SimpleGoX separates the UI from all security-critical operations. The WebView (frontend) handles display only. All Matrix logic, encryption, and key management run natively in Rust, in a separate process that the WebView cannot access.

**Frontend (WebView)** - HTML/CSS/JS, display only, no access to keys or tokens.

**Tauri Commands (IPC)** - The only bridge between frontend and backend. Passes serialized data, never key material.

**Rust Backend** - sgx-core wraps matrix-sdk and vodozemac. Crypto runs here natively. Keys never leave this process.

## Encryption

All encryption is handled by [vodozemac](https://github.com/matrix-org/vodozemac), the same library used by Element X, audited by Least Authority. SimpleGoX uses it natively through [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk), not compiled to WASM.

| Feature | Status |
|:--------|:-------|
| Olm/Megolm (E2E encryption) | Working |
| Cross-signing (MSC4153) | Working |
| Key backup with recovery key | Working |
| Device verification (SAS) | Working |

---

## Features

- End-to-end encrypted messaging (Olm/Megolm via vodozemac)
- Cross-signing with recovery key
- Federation (tested: simplego.dev <-> matrix.org)
- Room list with encryption indicators
- Live message sending and receiving
- Typing indicators (send and receive)
- Read receipts
- Message grouping and date separators
- Sender colors
- Settings screen with privacy controls
- Dark theme

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
│   ├── sgx-core/               # Shared Matrix client logic
│   ├── sgx-terminal/           # CLI client
│   └── sgx-iot/                # IoT tools
├── src-tauri/                  # Tauri Rust backend
│   └── src/
│       ├── lib.rs              # App entry point
│       └── commands.rs         # IPC command handlers
├── src/                        # Web frontend
│   ├── index.html
│   ├── js/app.js
│   └── styles/main.css
└── docs/
    └── public/                 # Documentation
```

---

## SimpleGoX ecosystem

| Project | Description | Repository |
|:--------|:------------|:-----------|
| **SimpleGoX** | Desktop client and core library | [GitHub](https://github.com/saschadaemgen/SimpleGoX) |
| **SimpleGoX Chat** | E2E-encrypted website chat widget | [GitHub](https://github.com/saschadaemgen/SimpleGoX-Chat) |
| **SimpleGoX ESP** | ESP32 Matrix IoT devices | [GitHub](https://github.com/saschadaemgen/SimpleGoX-ESP) |

SimpleGoX is the successor to [SimpleGo](https://github.com/saschadaemgen/SimpleGo) (ESP32 hardware messenger) and [GoChat](https://github.com/saschadaemgen/GoChat) (browser chat widget), now built on the Matrix protocol instead of SimpleX.

---

## Homeserver

SimpleGoX includes deployment support for [Tuwunel](https://github.com/matrix-construct/tuwunel), a lightweight Rust-based Matrix homeserver. The project maintains a public instance at `matrix.simplego.dev` for development and testing.

SimpleGoX works with any Matrix homeserver (Synapse, Dendrite, Tuwunel, Conduit).

---

## Roadmap

| Phase | Focus | Status |
|:------|:------|:-------|
| Season 1 | Foundation - core client, homeserver, federation, desktop GUI | In progress |
| Season 2 | Polish - Svelte migration, notifications, multi-account | Planned |
| Season 3 | Raspberry Pi image (minimal Linux) | Planned |
| Season 4 | Hardened Linux (Buildroot/Yocto, read-only, encrypted storage) | Planned |
| Season 5+ | Custom hardware with secure elements | Future |

---

## License

Apache-2.0

## Acknowledgments

[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) (Matrix client SDK) - [vodozemac](https://github.com/matrix-org/vodozemac) (Olm/Megolm cryptography) - [Tauri](https://v2.tauri.app) (desktop framework) - [Tuwunel](https://github.com/matrix-construct/tuwunel) (homeserver)

---

<p align="center">
  <i>SimpleGoX is an independent open-source project by <a href="https://it-and-more.systems">IT and More Systems</a>, Recklinghausen, Germany.</i>
</p>
