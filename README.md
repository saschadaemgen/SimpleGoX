<p align="center">
  <img src=".github/assets/github_header.png" alt="SimpleGoX" width="100%">
</p>

<p align="center">
  <h1 align="center">SimpleGoX</h1>
</p>

<p align="center">
  <strong>Privacy-first multi-messenger platform unifying Matrix, Telegram, SimpleX and WhatsApp in one secure inbox.</strong><br>
  Free desktop client with native Tor and I2P routing built in, plus dedicated hardware messenger devices with secure elements and verified boot.<br>
  Built with Rust, Tauri v2 and Svelte. All cryptography runs natively - encryption keys never touch the WebView.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Status-0.0.2--pre--alpha-red.svg" alt="Pre-Alpha">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0--or--later-blue.svg" alt="License"></a>
  <a href="https://matrix.org"><img src="https://img.shields.io/badge/Protocol-Matrix-black.svg" alt="Matrix"></a>
  <a href="https://core.telegram.org"><img src="https://img.shields.io/badge/Protocol-Telegram-blue.svg" alt="Telegram"></a>
  <img src="https://img.shields.io/badge/Protocol-SimpleX-darkblue.svg" alt="SimpleX">
  <img src="https://img.shields.io/badge/Protocol-WhatsApp-25D366.svg" alt="WhatsApp">
  <img src="https://img.shields.io/badge/Routing-Tor-purple.svg" alt="Tor">
  <img src="https://img.shields.io/badge/Routing-I2P-darkgreen.svg" alt="I2P">
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Built_with-Rust-orange.svg" alt="Rust"></a>
  <a href="https://v2.tauri.app"><img src="https://img.shields.io/badge/Desktop-Tauri_v2-blue.svg" alt="Tauri"></a>
</p>

---

## ⚠ Read this first - Pre-Alpha Development Notice

**SimpleGoX is being developed in the open as a pre-alpha project. What you see here is a live construction site, not a finished product.**

What this means in practice:

- **Features can change in minutes.** What works today may be restructured tomorrow. APIs, UI, data formats, and protocols are all subject to change without notice.
- **Just because something works does not mean it works correctly.** A message arriving at its destination is not the same as a feature being production-ready. Edge cases, reconnect behavior, error handling, persistence across restarts - all of these are still being hardened.
- **The app is not officially usable yet.** We do NOT recommend using SimpleGoX for real conversations you depend on. It is not ready to replace your primary messenger. It will break. You will lose messages.
- **Security review is ongoing.** The architecture is designed for security, but none of the protocol implementations have undergone external audit yet. Do not trust this client with information that actually needs protecting.
- **The first Beta release will mark the point where all advertised features are integrated and expected to function.** The first Stable release will mark the point where the product is officially usable. Neither of these has happened yet.

If you are a developer, security researcher, or enthusiast who wants to follow the development, test early versions, or contribute - you are very welcome. If you are looking for a messenger to use daily, please wait for the first Stable release.

This is how software built to be trusted gets built. Publicly. With mistakes visible. With time to get things right before anyone relies on them.

---

## What is SimpleGoX?

SimpleGoX is a multi-messenger platform designed around one principle: **your private conversations should stay private, including the metadata about who you are talking to and how you are reaching them.**

All your chats from Matrix, Telegram, SimpleX and WhatsApp appear in a single list, sorted by last activity. You never switch between apps. But the unified inbox is only the visible surface - the architecture underneath is built for privacy:

- **Native Tor routing.** One click and your Matrix traffic travels through the Tor network with onion routing and exit IP verification. No external Tor Browser needed, no SOCKS configuration, no separate installation.
- **Native I2P routing.** One click and your traffic enters the invisible I2P network with garlic routing and zero exit nodes. Traffic never leaves the encrypted overlay. No other messenger ships I2P integration at this level.
- **Native cryptography outside the browser.** Signal Desktop and Element Desktop run their cryptographic code as JavaScript inside Chromium. SimpleGoX runs vodozemac (for Matrix) and native Rust implementations (for SimpleX) in memory the WebView cannot touch. Your encryption keys never enter the browser context.
- **Process isolation per protocol.** Each messenger protocol runs as its own isolated OS process. Your Telegram keys live in the Telegram process. Your Matrix keys live in the Tauri backend. Your SimpleX keys live in the SimpleX sidecar. No shared memory, no cross-contamination.
- **No server-side bridges.** Unlike Beeper which bridges everything through Matrix on a cloud server, SimpleGoX connects to each protocol natively on your client. Nothing gets proxied. Nothing leaves your device unencrypted.

SimpleGoX is the first multi-messenger to integrate both Tor and I2P at the client level, and the first multi-messenger to run vodozemac cryptography outside the WebView. It is also the first Rust-native SimpleX client speaking SMP v9 to unmodified SimpleX relays without a Haskell runtime.

The long-term vision goes beyond desktop: SimpleGoX is designed to run on dedicated hardware terminals with secure elements, tamper detection and hardened Linux - a physical messenger device you own and control. See [ARCHITECTURE_AND_SECURITY.md](ARCHITECTURE_AND_SECURITY.md) for the complete security architecture spanning from silicon to user interface.

### The SimpleGoX ecosystem

| Product | Description | Status |
|:--------|:------------|:-------|
| **SimpleGoX Desktop** | Multi-messenger client for Windows, Linux and macOS | Pre-alpha |
| **SimpleGoX Hardware** | Dedicated messenger devices in 3 security classes | Planning |
| **[SimpleGoX Chat](https://github.com/saschadaemgen/goChatX)** | Embeddable E2E-encrypted website chat widget | In development |
| **[SimpleGoX ESP](https://github.com/saschadaemgen/SimpleGoX-ESP)** | ESP32 IoT devices that speak Matrix natively | In development |

SimpleGoX is the successor to [SimpleGo](https://github.com/saschadaemgen/SimpleGo) (ESP32 hardware messenger) and [GoChat](https://github.com/saschadaemgen/GoChat) (browser chat widget), now built on the Matrix protocol with multi-messenger support.

---

## Privacy and Security Features

### Built-in anonymity networks

Most messengers leave you to figure out Tor or I2P on your own, if they support them at all. SimpleGoX integrates both directly:

- **Tor via Arti 0.41** - pure Rust Tor implementation, no external daemon. Exit IP verification built in. Live dashboard shows circuit state.
- **I2P via i2pd 2.56.0** - garlic routing with zero exit nodes. Live dashboard shows routers, tunnels, bandwidth in real time.
- **Switching is seamless** - sync cancels cleanly, proxy changes, new sync starts. No manual restart needed.
- **Per-protocol routing** - you can route Matrix via Tor while running Telegram direct, or any other combination.

### Isolation beyond standard sandboxing

- **Keys outside the browser.** Matrix encryption uses vodozemac in native Rust. SimpleX encryption uses our own Rust implementation. Neither touches the WebView. If Chromium has a zero-day, your keys are in a different process.
- **Per-protocol processes.** Each protocol backend runs as its own binary. Inter-process communication happens only over localhost gRPC with a strictly defined protobuf contract.
- **Deny-by-default IPC.** Every Tauri command must be explicitly registered and permitted. Capabilities, permissions and scopes form three layers of access control. No frontend code can reach the filesystem or network without explicit allow-list entries.

### Native cryptography stack

- **Matrix:** vodozemac (Olm + Megolm) running natively in Rust
- **SimpleX:** SMP v9 with X448 X3DH, Double Ratchet, NaCl cryptobox, custom AES-256-GCM with 16-byte IV, ready for SNTRUP761 post-quantum KEM
- **Telegram:** TDLib 1.8.61 isolated in its own sidecar process
- **WhatsApp:** Signal Protocol via EU DMA interoperability (planned)

### What we do not do

- **No telemetry.** Zero analytics, zero crash reporting, zero phone-home.
- **No cloud services.** No SimpleGoX servers are involved in your conversations.
- **No account on our side.** You do not register with us. You register with Matrix, Telegram, SimpleX relays directly.
- **No vendor lock-in.** All protocols are standards you can connect to with other clients.

### What you should understand about metadata

Strong end-to-end encryption protects message content, but **metadata is harder**. Which server you connect to, at what times, from which IP - this is visible to network observers unless you route through Tor or I2P. SimpleGoX gives you the tools to minimize metadata exposure, but you still have to use them. Reading [ARCHITECTURE_AND_SECURITY.md](ARCHITECTURE_AND_SECURITY.md) is recommended if you care about the full threat model.

---

## Protocols

| Protocol | Status | Implementation |
|:---------|:-------|:---------------|
| **Matrix** | Working | Native Rust via matrix-rust-sdk 0.16 + vodozemac |
| **Telegram** | Working | TDLib 1.8.61 via tdlib-rs in isolated sidecar process |
| **SimpleX** | Bidirectional chat working | Native Rust SMP v9 via sgx-simplex sidecar. Full handshake, bidirectional E2EE messaging against unmodified SimpleX Desktop. First Rust-native SimpleX client not requiring a Haskell runtime. Reconnect hardening in progress. |
| **WhatsApp** | Planned | Official EU DMA interoperability path |

## Routing

| Mode | Status | Implementation |
|:-----|:-------|:---------------|
| **Direct** | Working | Standard HTTPS connection to homeserver |
| **Tor** | Working | Native Arti 0.41 (Rust Tor) with exit IP verification |
| **I2P** | Working | i2pd 2.56.0 (C++) sidecar with hidden service routing |

---

## How SimpleGoX differs from alternatives

**vs Beeper / mautrix bridges:** Beeper bridges every protocol through Matrix on a cloud server. Your Telegram messages pass through their infrastructure where they could theoretically be intercepted. SimpleGoX bridges nothing - each protocol connects natively from your client.

**vs Signal Desktop / Element Desktop:** Both run Electron, which bundles a full Chromium browser and executes cryptographic code as JavaScript inside it. In July 2024, researchers found Signal Desktop had stored its database encryption key as plaintext in a JSON file for six years. In September 2025, Trail of Bits demonstrated CVE-2025-55305 allowing any Electron app to be silently backdoored via V8 heap snapshots. SimpleGoX uses Tauri v2 - native Rust, shared system WebView, deny-by-default IPC.

**vs standard messenger apps with "optional Tor":** Most messengers that "support" Tor require you to route them through the Tor Browser or configure SOCKS5 manually. SimpleGoX ships Arti (the Rust Tor implementation) as part of the app. One click, done.

**vs all other messengers:** No other messenger ships I2P integration. SimpleGoX is the first.

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
        |       |
        |       +-- Direct (HTTPS to homeserver)
        |       +-- Tor (Arti 0.41, SOCKS5 on port 19150)
        |       +-- I2P (i2pd sidecar, SOCKS5 on port 4447)
        |
        +-- gRPC Client --> sgx-telegram Sidecar (TDLib)
        |
        +-- gRPC Client --> sgx-simplex Sidecar (native Rust SMP v9)
        |
        +-- gRPC Client --> sgx-whatsapp Sidecar (planned)
```

**Frontend (Svelte 5)** - UI only, no access to keys or tokens.

**Tauri Backend (Rust)** - Matrix client runs in-process with configurable routing (Direct, Tor, I2P). External protocols connect via gRPC to isolated sidecar binaries.

**Routing Layer** - Tor runs natively via Arti (Rust). I2P runs as an i2pd sidecar process with built-in SOCKS5 proxy. Mode switching is seamless - sync cancels cleanly, proxy changes, new sync starts. StatusBanner shows live progress with expandable log.

**Sidecar Processes** - Each non-Matrix protocol runs as a separate binary. Communicates over localhost gRPC using a shared protobuf schema (messenger.proto). Complete process isolation. The Telegram sidecar auto-starts on app launch when a saved session exists. The SimpleX sidecar auto-starts when a profile is configured.

**Protobuf Contract** - All sidecars implement the same `MessengerService` gRPC interface. Adding a new protocol means implementing one more sidecar binary against this contract. The frontend and Tauri backend remain untouched.

---

## Features

### Messaging
- End-to-end encrypted messaging (Olm/Megolm via vodozemac for Matrix, SMP v9 Double Ratchet for SimpleX)
- Cross-signing with recovery key (Matrix)
- Federation (tested: simplego.dev <-> matrix.org)
- Telegram message sending and receiving
- SimpleX bidirectional chat with unmodified SimpleX Desktop clients
- Unified chat list across all four protocols
- Message grouping with stacked bubbles
- Reply, reactions, edit, redact (Matrix)
- Emoji and animated emoji support (Telegram)
- Date separators (Today, Yesterday, full date)

### Multi-Messenger
- Protocol badges (MX, TG, SX) on every chat
- Chat type badges (Grp, Ch) for Telegram groups and channels
- Single sorted inbox across all protocols
- Isolated sidecar architecture per protocol
- Automatic sidecar startup and session restore
- Account management per protocol (connect, disconnect)
- Per-protocol auto-connect gated on session presence (no wasted retry loops on fresh installs)

### Setup Wizard
- First-launch guided setup with per-protocol confirmation screens
- SimpleX first-class option (profile, bio, SMP server selection)
- Matrix login with homeserver selection
- Telegram authentication (phone, code, 2FA)
- Skip-capable with at-least-one-messenger constraint
- Auto-advancing completion screen for multi-protocol setups

### Privacy Routing
- Tor integration via Arti (Rust implementation)
- I2P integration via i2pd sidecar
- Per-protocol routing configuration
- Live dashboards for both Tor and I2P
- Exit IP verification for Tor
- Zero-exit-node guarantee for I2P

### UI
- Dark theme with user-customizable accent color
- Visual color picker (2D saturation/lightness field plus hue slider, 10 presets)
- Animated background (aurora plus particles)
- Branded splash screen
- StatusBanner with expandable terminal-style log
- Custom two-part bubble design with avatars, reply quotes, reactions
- Emoji picker for Matrix reactions

---

## Prerequisites

### Windows
- Rust (via rustup) with the latest stable toolchain
- Node.js 18 or newer
- WebView2 Runtime (pre-installed on Windows 11, evergreen on Windows 10)
- protoc (Protocol Buffers compiler): `winget install protobuf` or download from [protobuf releases](https://github.com/protocolbuffers/protobuf/releases)

### Linux
- Rust (via rustup) with the latest stable toolchain
- Node.js 18 or newer
- System dependencies: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- protoc: `sudo apt install protobuf-compiler`

### macOS
- Xcode Command Line Tools (`xcode-select --install`)
- protoc via Homebrew: `brew install protobuf`

### I2P Binary

The I2P integration requires the i2pd binary. Download from [PurpleI2P/i2pd releases](https://github.com/PurpleI2P/i2pd/releases) and place it at:
- **Windows:** `%LOCALAPPDATA%\dev.simplego.app\i2pd.exe`
- **Linux:** Install via `sudo apt install i2pd` or place binary in PATH

Note: Windows Defender may flag i2pd as a PUA (Potentially Unwanted Application). This is a false positive common with anonymization tools. Add `%LOCALAPPDATA%\dev.simplego.app\` to your Defender exclusions.

### Development

```bash
cd SimpleGoX
npm install
cargo tauri dev
```

The Telegram sidecar starts automatically when a saved session exists. The SimpleX sidecar starts automatically when a profile is configured. For first-time setup, the wizard guides you through the authentication flow for each protocol.

**Note:** After `cargo clean`, you may need to copy `tdjson.dll` manually:
```powershell
Copy-Item "target\debug\build\tdlib-rs-*\out\tdlib\bin\tdjson.dll" "target\debug\"
```

### Production build

```bash
cargo tauri build
```

Output: `.msi` (Windows), `.deb`/`.AppImage` (Linux), or `.dmg` (macOS) in `target/release/bundle/`

---

## Project structure

```
SimpleGoX/
├── crates/
│   ├── sgx-core/               # Matrix client (matrix-rust-sdk wrapper)
│   ├── sgx-proto/              # Shared protobuf/gRPC definitions
│   ├── sgx-simplex/            # SimpleX sidecar (native Rust SMP v9)
│   └── sgx-telegram/           # Telegram sidecar (TDLib + gRPC server)
├── proto/
│   └── messenger.proto         # Unified messenger service contract
├── src-tauri/                  # Tauri v2 Rust backend
│   └── src/
│       ├── lib.rs              # App entry, sidecar auto-start, tracing
│       ├── commands.rs         # Matrix IPC commands
│       ├── routing_commands.rs # Routing mode switching (Direct/Tor/I2P)
│       ├── telegram_commands.rs # Telegram IPC commands
│       ├── simplex_commands.rs # SimpleX IPC commands
│       ├── tor.rs              # Arti Tor integration
│       ├── i2p.rs              # i2pd sidecar management
│       ├── tor_logging.rs      # Tor log forwarding
│       └── sidecar.rs          # Sidecar process manager
├── src/                        # Svelte 5 frontend
│   ├── components/
│   │   ├── ChatLayout.svelte   # Main layout with sidebar and chat
│   │   ├── ChatView.svelte     # Message display with custom bubbles
│   │   ├── RoomList.svelte     # Unified chat sidebar
│   │   ├── RoomItem.svelte     # Individual chat entry with badges
│   │   ├── Avatar.svelte       # Avatar with MXC and TG file support
│   │   ├── StatusBanner.svelte # Global status with expandable log
│   │   ├── Settings.svelte     # Fullscreen settings overlay
│   │   ├── SplashScreen.svelte # Branded splash on app startup
│   │   ├── AnimatedBackground.svelte # Aurora + particle effects
│   │   ├── TelegramAuth.svelte # Telegram login flow
│   │   ├── SimplexProfileDialog.svelte # SimpleX profile setup
│   │   ├── wizard/             # Setup wizard components
│   │   ├── settings/           # Settings tab components
│   │   └── ui/                 # Shared UI components (Tooltip, ColorPicker)
│   ├── lib/
│   │   ├── stores.js           # Svelte reactive stores
│   │   └── tauri.js            # Tauri command wrappers
│   └── App.svelte
├── scripts/                    # Development helper scripts
├── ARCHITECTURE_AND_SECURITY.md # Security whitepaper
├── CHANGELOG.md
└── docs/
    └── internal/               # Season protocols, briefings (gitignored)
```

---

## Homeserver

SimpleGoX works with any Matrix homeserver (Synapse, Dendrite, Tuwunel, Conduit). The project maintains a public instance at `matrix.simplego.dev` running [Tuwunel](https://github.com/matrix-construct/tuwunel) 1.5.1 for development and testing.

The homeserver is also reachable via I2P hidden service for fully anonymous connections.

---

## Security

SimpleGoX is designed as a complete security system from silicon to user interface. The desktop client provides native Rust cryptography outside the WebView with per-protocol process isolation. Traffic can be routed through Tor (onion routing with exit IP verification) or I2P (garlic routing with zero exit nodes) with one click.

For the complete security architecture including hardware classes, post-quantum cryptography plans, secure deletion mechanisms, competitor comparison, and certification roadmap, see [ARCHITECTURE_AND_SECURITY.md](ARCHITECTURE_AND_SECURITY.md).

---

## Roadmap

| Phase | Focus | Status |
|:------|:------|:-------|
| Desktop Client Foundation | Matrix + Telegram multi-messenger, setup wizard, UI foundation | Done |
| Privacy Routing | Tor and I2P integration with live dashboards | Done |
| SimpleX Integration | Native Rust SMP v9, bidirectional chat, UI integration | Done (hardening in progress) |
| Connection Hardening | Reconnect logic, keep-alive, session recovery | In progress |
| Context Menus and Management | Contact delete, archive, block, mute per protocol | In progress |
| WhatsApp Integration | EU DMA interoperability, template system | Planned |
| Beta Release | All advertised features integrated and expected to function | Planned |
| Stable Release | Product officially usable, external security audit complete | Planned |
| Hardware Class 1 | Raspberry Pi image, dedicated terminal | Planned |
| Hardware Class 2 | Custom PCB with NXP i.MX 93, dual secure elements | Future |
| Hardware Class 3 | Vault edition with triple secure elements, tamper detection | Future |

---

## Contributing

SimpleGoX is in active pre-alpha development and contributions are welcome. Whether it is bug reports, feature ideas, code contributions or documentation improvements - every bit helps.

Please use [Conventional Commits](https://www.conventionalcommits.org/) for all commits: `type(scope): description`

Examples: `feat(telegram): add message pagination`, `fix(ui): settings border color`, `docs(readme): add build prerequisites`

---

## Development philosophy

SimpleGoX is built in the open because software that handles your private conversations needs to be inspectable. Every protocol decision, every security trade-off, every architectural choice is documented and reviewable. The CHANGELOG tracks every meaningful change by Season.

We do not ship "move fast and break things" in a messenger. We ship "move carefully, document everything, break in the open where everyone can see it."

---

## License

AGPL-3.0-or-later

## Acknowledgments

[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) (Matrix client SDK) - [vodozemac](https://github.com/matrix-org/vodozemac) (Olm/Megolm cryptography) - [Tauri](https://v2.tauri.app) (desktop framework) - [tdlib-rs](https://github.com/FedericoBruzzone/tdlib-rs) (Telegram Database Library wrapper) - [Tuwunel](https://github.com/matrix-construct/tuwunel) (homeserver) - [Arti](https://gitlab.torproject.org/tpo/core/arti) (Rust Tor implementation) - [i2pd](https://github.com/PurpleI2P/i2pd) (C++ I2P implementation) - [SimpleX Chat](https://simplex.chat) (SimpleX protocol specification and reference implementations)

---

<p align="center">
  <i>SimpleGoX is an independent open-source project by <a href="https://it-and-more.systems">IT and More Systems</a>, Recklinghausen, Germany.</i>
</p>

<p align="center">
  <a href="https://simplego.dev">simplego.dev</a>
</p>
