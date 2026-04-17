# SimpleGoX - Architecture & Security

**Document version:** April 2026
**Project:** SimpleGoX Multi-Messenger Platform
**License:** AGPL-3.0-or-later
**Copyright:** 2025-2026 Sascha Daemgen, IT and MORE Systems, Recklinghausen

---

## What is SimpleGoX?

SimpleGoX is a multi-messenger platform that unifies Matrix, Telegram, SimpleX, and WhatsApp into a single secure application. Unlike conventional messenger apps that run on general-purpose operating systems, SimpleGoX is designed from the ground up as a complete security system spanning from silicon to user interface.

The platform ships in two forms:

**Software:** A free, open-source desktop application for Windows, Linux, and macOS built with Tauri v2 and Rust. All cryptographic operations run natively outside the WebView, making it structurally more secure than Electron-based alternatives like Signal Desktop or Element Desktop.

**Hardware:** Purpose-built communication devices in three classes, running a minimal verified-boot Linux that exists solely to execute SimpleGoX. No desktop environment, no browser, no package manager. The device IS the messenger.

This document describes the complete security architecture across all product variants.

---

## 1. Software Architecture

### 1.1 Why Tauri v2, not Electron

Every major desktop messenger today uses Electron: Signal Desktop, Element Desktop, Slack, Discord. Electron bundles a full Chromium browser engine plus Node.js into every application, creating a massive attack surface.

In July 2024, researchers discovered that Signal Desktop had stored its SQLCipher database encryption key as plaintext in a JSON configuration file for six years. Any process running as the same user could read it. In September 2025, Trail of Bits published CVE-2025-55305, demonstrating that Chromium's V8 heap snapshot feature could be exploited to silently backdoor Signal, 1Password, Slack, and any other Electron application installed to user-writable directories.

These are not implementation bugs. They are structural consequences of the Electron architecture.

Tauri v2 takes a fundamentally different approach:

| Property | Electron | Tauri v2 |
|---|---|---|
| Rendering engine | Bundled Chromium (per app) | System WebView (shared, OS-patched) |
| Backend language | JavaScript (Node.js) | Rust (memory-safe, compiled) |
| Binary size | 50-165 MB | 2.5-10 MB |
| Idle RAM | 100-300+ MB | 30-40 MB |
| Crypto location | In-process JavaScript | Native Rust, outside WebView |
| IPC model | Open by default | Deny by default |
| Security audit | None publicly available | Radically Open Security, Aug 2024 |

The Radically Open Security audit (November 2023 through August 2024, funded by NLNet/NGI) examined Tauri v2 before its stable release. All 21 findings (11 High, 2 Elevated, 3 Moderate, 5 Low) were resolved before launch.

### 1.2 Tauri v2 Security Model

Tauri v2 separates every application into two zones:

**Trusted Zone (Rust backend):** Has access to the filesystem, network, hardware, and cryptographic operations. Written in Rust, which eliminates entire vulnerability classes (buffer overflows, use-after-free, data races) at compile time.

**Untrusted Zone (WebView frontend):** Renders the user interface. Has ZERO access to anything outside its sandbox by default. Cannot read files, cannot open network connections, cannot access the OS clipboard without explicit permission.

Communication between zones happens through a strictly validated IPC bridge. The frontend calls named Rust functions via `invoke()`. Each function must be explicitly registered and permitted through three layers of access control:

**Capabilities** define which windows can access which commands. A window not matching any capability has zero IPC access.

**Permissions** are command-level toggles. Each Tauri command can be individually enabled or disabled per capability.

**Scopes** restrict the parameters a command can accept. For example, a file-read permission can be scoped to only allow reading files within a specific directory.

This deny-by-default model has no equivalent in Electron.

### 1.3 Protocol Isolation Architecture

SimpleGoX runs four messenger protocols simultaneously. Each protocol has fundamentally different trust models, key formats, and encryption schemes. Mixing their key material would be catastrophic.

**Matrix** uses Olm (Double Ratchet) for 1:1 chats and Megolm (group ratchet) for rooms, implemented in Vodozemac (Rust). Keys are Curve25519/Ed25519. End-to-end encrypted by default.

**Telegram** uses MTProto 2.0 with a 2048-bit authorization key and AES-256-IGE. Critical limitation: regular "cloud chats" are only client-server encrypted. The server holds keys and can read messages. Only "Secret Chats" are end-to-end encrypted. SimpleGoX makes this distinction visible to the user.

**SimpleX** uses the SMP (SimpleX Messaging Protocol) with per-queue ephemeral Curve25519 keys, Double Ratchet with X448, and NaCl cryptobox. It has no user identifiers of any kind, providing the strongest metadata protection of any messenger protocol. SimpleX has already integrated sntrup761 post-quantum KEM into every ratchet step.

**WhatsApp** (via EU DMA interoperability) uses the Signal Protocol with Curve25519 identity keys and client-fanout encryption for multi-device support.

SimpleGoX enforces strict isolation between protocols:

```
+------------------+     +------------------+
|  Matrix Worker   |     | Telegram Sidecar |
|  (sgx-core)      |     | (sgx-telegram)   |
|  Rust in-process  |     | Separate process  |
|  Own SQLite DB   |     | Own TDLib SQLite   |
|  Own key store   |     | Own key store     |
+--------+---------+     +--------+---------+
         |                         |
         +--- Unix Socket/gRPC ---+
         |                         |
+--------+---------+     +--------+---------+
| SimpleX Worker   |     | WhatsApp Worker  |
| Separate process  |     | Separate process  |
| Own key store    |     | Own key store     |
+------------------+     +------------------+
         |                         |
         +----------+-------------+
                    |
            +-------+--------+
            | Tauri IPC      |
            | Broker         |
            | (Rust core)    |
            +-------+--------+
                    |
            +-------+--------+
            | WebView UI     |
            | (Svelte 5)     |
            | No key access  |
            +----------------+
```

Each protocol handler runs as a separate OS process with:

- Its own PID namespace (cannot see other processes)
- Its own mount namespace (cannot access other protocol's files)
- Its own user namespace (runs as a separate unprivileged user)
- A seccomp-BPF filter restricting syscalls to the minimum required
- cgroups v2 enforcing memory, CPU, and process count limits

Key derivation uses domain separation to ensure no cryptographic material is ever shared:

```
User Master Key (from Secure Element or OS keyring)
    |
    +--- HKDF(master, "sgx-matrix-v1")  --> Matrix key store
    +--- HKDF(master, "sgx-telegram-v1") --> Telegram key store
    +--- HKDF(master, "sgx-simplex-v1")  --> SimpleX key store
    +--- HKDF(master, "sgx-whatsapp-v1") --> WhatsApp key store
```

A compromise of one protocol's keys has ZERO impact on the others.

### 1.4 Desktop Key Storage

On desktop platforms, SimpleGoX uses the OS-native credential storage to protect encryption keys:

**Windows:** DPAPI (Data Protection API) encrypts secrets with the user's login credentials. The encrypted blob is stored in the application data directory. While any process running as the same user can decrypt via DPAPI, this matches the OS trust model.

**macOS:** The Keychain provides per-application access control lists. Other applications cannot read SimpleGoX's keychain entries without explicit user authorization. This is the strongest desktop key storage model available.

**Linux:** The Secret Service API (GNOME Keyring or KDE Wallet) provides session-level secret storage. SimpleGoX detects whether a secret store is available at startup. If none is found, it generates a random encryption key and warns the user that key protection is limited to file permissions.

On all platforms, the OS keyring stores only a database encryption key. This key decrypts the local SQLite databases containing session tokens, encryption keys, and cached messages. The databases are never stored in plaintext on disk.

### 1.5 Secure Cross-Protocol Sharing

Users can forward messages, images, links, and videos between protocols. This requires careful handling to prevent data leaks:

**In-memory pipeline:** When a user shares a Matrix message to a Telegram chat, the plaintext message travels through Rust memory channels (`tokio::sync::mpsc`) wrapped in `Zeroizing<Vec<u8>>` containers. The message is re-encrypted by the target protocol's handler before transmission. Plaintext never touches the filesystem or OS clipboard.

**Clipboard isolation:** SimpleGoX implements auto-clearing: any text or media placed on the clipboard is automatically removed after 15 seconds. On Wayland (modern Linux), clipboard access is restricted to the focused application. On X11, SimpleGoX warns users that any application can read the clipboard.

**Media forwarding:** Media files (images, videos, documents) are decrypted in Rust memory, streamed to the target protocol handler for re-encryption, and displayed in the WebView via Blob URLs. Blob URLs exist only in memory and are garbage-collected when the reference is released.

### 1.6 Network Anonymization Layer (Tor and I2P)

SimpleGoX integrates two independent anonymization networks directly into the application. Tor runs natively as a Rust library (Arti) inside the Tauri backend. I2P runs as an i2pd sidecar process managed by the application. No external Tor Browser, no manual configuration - privacy routing is one click away.

#### 1.6.1 Per-Protocol Routing

Each messenger protocol can independently select its transport mode:

| Protocol | Direct | Tor | I2P | Rationale |
|---|---|---|---|---|
| Matrix | Yes | Yes | Yes | Own homeserver supports both Tor and I2P hidden services |
| Telegram | Yes | Yes | No | Telegram servers are on the clearnet, I2P has no exit nodes |
| SimpleX | Yes | Yes | Yes | SMP relays can run as I2P hidden services |
| WhatsApp | Yes | Yes | No | WhatsApp servers are on the clearnet, I2P has no exit nodes |

The routing selection is mutually exclusive per protocol. A protocol cannot use Tor and I2P simultaneously. This prevents circuit correlation attacks where an adversary controlling both a Tor exit node and an I2P outproxy could deanonymize traffic.

#### 1.6.2 Embedded Tor via Arti

SimpleGoX embeds Arti 0.41, the official Tor implementation in Rust developed by the Tor Project. Arti replaces the legacy C tor daemon with a memory-safe, embeddable library.

The TorManager component handles the complete lifecycle:

**Bootstrap:** Arti connects to the Tor network using cached directory information when available (reducing startup from 30 seconds to under 10 seconds on subsequent launches). The bootstrap process downloads consensus documents from directory authorities, selects guard nodes, and builds initial circuits.

**SOCKS5 Proxy Bridge:** A custom proxy bridge listens on 127.0.0.1:19150. The bridge accepts SOCKS5 connections from reqwest (the HTTP client used by matrix-rust-sdk) and routes them through Arti's Tor circuits. A critical implementation detail: the Arti DataStream requires explicit flush() calls after each write operation, without which the connection appears to hang indefinitely. This was discovered during development and is not documented in Arti's API reference.

**Per-Protocol Circuit Isolation:** Each protocol's traffic is routed through independent Tor circuits. Matrix traffic and Telegram traffic never share the same circuit, preventing a malicious exit node from correlating traffic between protocols.

**Connection Timeouts:** Tor connections require significantly longer timeouts than direct connections due to the multi-hop relay chain. SimpleGoX uses:
- reqwest connect timeout: 60 seconds (vs. 10 seconds direct)
- reqwest request timeout: 120 seconds (vs. 30 seconds direct)
- matrix-rust-sdk RequestConfig: 120 seconds
- SyncSettings timeout: 60 seconds with automatic retry on timeout

**Exit IP Verification:** After bootstrap, SimpleGoX automatically verifies the Tor exit IP via api.ipify.org to confirm traffic is genuinely routing through Tor. The exit IP is displayed in the Tor Dashboard.

**Persistence:** The routing configuration is saved to the application data directory. On app restart, the saved configuration is loaded and the appropriate anonymization network is automatically bootstrapped before the messenger protocols connect. This ensures no clearnet traffic leaks during startup.

#### 1.6.3 I2P via i2pd Sidecar

SimpleGoX uses i2pd 2.56.0, the mature C++ implementation of the I2P protocol stack (maintained since 2016), as an external sidecar process. The initial plan was to embed emissary-core (a pure Rust I2P implementation), but testing revealed three critical bugs in emissary v0.4.0: a duration overflow panic on second launch, self-shutdown after ~9 minutes, and a transit tunnel panic after ~25 minutes. All three bugs were reported upstream (Issues #339, #340, #341). A fork with a fix for the first bug was created at github.com/saschadaemgen/emissary. The decision was made to use i2pd for stability while monitoring emissary's development.

**Architecture Differences from Tor:**

| Property | Tor (Arti) | I2P (i2pd) |
|---|---|---|
| Integration | Native Rust library (in-process) | C++ sidecar process (managed) |
| Routing model | Onion routing (bidirectional circuits) | Garlic routing (unidirectional tunnels) |
| Exit traffic | Primary use case | Not supported |
| Bootstrap time | 10-30 seconds | 2-5 minutes (first run) |
| Round-trip latency | 200-600 ms | 1-3 seconds |
| Network size | 2+ million daily users | ~55,000 nodes |
| Tunnel lifetime | ~10 minutes (rotated) | 10 minutes (rebuilt) |
| UDP support | None | Native datagrams |
| Binary size | ~15 MB (compiled into app) | ~5 MB (external binary) |

**I2P Hidden Service for Matrix:** The SimpleGoX homeserver (matrix.simplego.dev) runs i2pd alongside Tuwunel, exposing Matrix as an I2P hidden service. The hidden service address is a .b32.i2p address derived from the server's cryptographic identity. When a user selects I2P routing for Matrix, the client connects to this .b32.i2p address through i2pd's built-in SOCKS5 proxy on port 4447. The connection uses HTTP (not HTTPS) because I2P provides its own end-to-end encryption at the transport layer, making TLS redundant.

**i2pd Process Management:** The application manages the i2pd lifecycle completely: binary discovery and placement in user data directory, process spawning with invisible window (CREATE_NO_WINDOW on Windows), SOCKS5 readiness detection, tunnel readiness verification via real SOCKS CONNECT to the homeserver, and clean shutdown on mode switch or app exit. Stale i2pd processes are killed on bootstrap to prevent port conflicts.

#### 1.6.4 Anonymization Limitations

**Tor limitations:** Telegram aggressively scores IP addresses and may freeze or ban accounts connecting from known Tor exit nodes. WhatsApp's certificate pinning and UDP requirements make Tor connections unreliable. SimpleGoX displays an experimental warning when users enable Tor for these protocols.

**I2P limitations:** The I2P network has approximately 55,000 nodes versus Tor's 2+ million daily users, providing a significantly smaller anonymity set. The 2-5 minute bootstrap time on first run requires clear progress indication. I2P's unidirectional tunnels with 10-minute lifetimes mean connections are periodically rebuilt, causing brief interruptions. The i2pd binary may be flagged as a PUA (Potentially Unwanted Application) by Windows Defender, requiring users to add exclusions.

**Shared limitation:** Neither Tor nor I2P protects against a global passive adversary that can observe all network traffic simultaneously. Both networks are vulnerable to traffic confirmation attacks where an adversary controls or observes both the entry and exit points of a connection.

### 1.7 Detailed Software Architecture

#### 1.7.1 Crate and Module Structure

The SimpleGoX codebase is organized as a Cargo workspace:

**sgx-core** (crates/sgx-core/): The Matrix protocol handler. Contains the SgxClient struct that wraps matrix-rust-sdk, providing session management, message sending/receiving, room operations, avatar handling, and the sync loop. All proxy configuration (Tor/I2P) and homeserver override logic lives here.

**sgx-proto** (crates/sgx-proto/): Protocol buffer definitions for the gRPC interface between the Tauri backend and the Telegram sidecar. Defines the MessengerService with RPCs for authentication, message retrieval, avatar downloads, update streaming, and proxy configuration.

**sgx-telegram** (crates/sgx-telegram/): The Telegram sidecar binary. Wraps TDLib via gRPC, providing message access, authentication, and real-time update streaming. Runs as a separate OS process to isolate Telegram's MTProto key material from the main application.

**sgx-simplex** (crates/sgx-simplex/): The SimpleX protocol sidecar. Implements the SMP v9 wire protocol natively in Rust without any Haskell dependencies. Handles TLS connections with fingerprint verification, the full v9 handshake (CbAuthenticator, X25519 session auth, version negotiation), queue lifecycle (NEW, SKEY, SUB, ACK), and the agent-level message format (AgentInvitation, AgentConfirmation). All cryptographic operations use the RustCrypto ecosystem (x25519-dalek, crypto_box, sha2) plus NaCl-compatible SalsaBox for the CbAuthenticator. Queue state is persisted in SQLite via rusqlite. The sidecar exposes a gRPC MessengerService on port 50053 for integration with the Tauri backend.

**src-tauri/**: The Tauri application core containing:
- `lib.rs` - Application setup, state management, sidecar lifecycle, auto-restore
- `commands.rs` - Matrix-related Tauri commands (login, sync, rooms, messages)
- `tor.rs` - TorManager, RoutingMode enum, ProtocolRouting, SOCKS5 proxy bridge
- `routing_commands.rs` - Routing-related Tauri commands (set protocol, check IP, save routing, I2P stats)
- `tor_logging.rs` - TorLogForwarder tracing Layer for UI log display
- `i2p.rs` - I2PManager, i2pd sidecar lifecycle (spawn, monitor, shutdown, stats parsing)
- `telegram_commands.rs` - Telegram gRPC client, message/avatar commands
- `sidecar.rs` - gRPC channel management with HTTP/2 keepalive

**src/** (Svelte 5 frontend): Component-based UI with reactive stores.

#### 1.7.2 Data Flow: Message Sending

When a user sends a Matrix message through Tor:

1. User types message in Svelte frontend
2. Frontend calls `invoke('send_message', { roomId, body })` via Tauri IPC
3. Tauri dispatches to the `send_message` Rust command
4. Command acquires SgxClient from AppState mutex
5. SgxClient calls matrix-rust-sdk's `room.send()` with the message event
6. matrix-rust-sdk serializes the request and passes it to reqwest
7. reqwest connects to the SOCKS5 proxy on 127.0.0.1:19150
8. The TorManager's proxy bridge receives the SOCKS5 CONNECT request
9. Arti establishes a Tor circuit to the destination
10. The HTTP request travels through 3 Tor relays to the homeserver
11. Tuwunel processes the request and distributes the message via federation
12. The response travels back through the same circuit
13. matrix-rust-sdk processes the response and updates local state
14. The sync loop emits a room timeline event
15. Frontend receives the event and updates the chat view reactively

#### 1.7.3 Inter-Process Communication

**Tauri IPC (Frontend to Backend):** Strictly validated, deny-by-default. Each command is registered in the invoke_handler and requires explicit capability grants. The frontend has zero direct access to the filesystem, network, or cryptographic operations.

**gRPC (Backend to Telegram Sidecar):** Tonic-based gRPC over localhost. The channel is configured with HTTP/2 keepalive (10s interval, 20s timeout), concurrency limit of 20, and TCP keepalive of 30 seconds. After any change to the proto definition, the sidecar binary must be recompiled or h2 protocol errors will occur.

**Tauri Events (Backend to Frontend):** Asynchronous event emission for real-time updates including tor-state, i2p-state, tor-exit-ip, tg-ready, tg-new-message, tg-message-edited, tg-message-deleted, and room-timeline-event.

#### 1.7.4 State Management

**Backend State (Rust):** All mutable state is wrapped in `tokio::sync::Mutex` and registered via Tauri's `.manage()` system: SgxClient (Matrix client, rebuilt when proxy changes), TorManager (Arti instance, SOCKS proxy, routing config), I2PManager (i2pd process handle, bootstrap state, stats cache), and the gRPC client handle for the Telegram sidecar connection.

**Frontend State (Svelte 5):** Reactive stores for routing configuration, selected room, cached Telegram chats, and a global avatar cache that is cleared on sidecar reconnect.

---

## 2. Cryptographic Foundation

### 2.1 Matrix Encryption: Vodozemac

The Matrix protocol's end-to-end encryption is implemented by Vodozemac, a Rust library developed by the Matrix.org Foundation. SimpleGoX uses Vodozemac natively through matrix-rust-sdk, not through JavaScript bindings.

Vodozemac implements two encryption protocols:

**Olm (1:1 chats):** Based on the Signal Protocol's Double Ratchet algorithm. Each message generates new encryption keys, providing forward secrecy (compromising current keys cannot decrypt past messages) and post-compromise security (the ratchet "heals" after a compromise).

**Megolm (group chats):** A group ratchet protocol where a single sender key encrypts messages for all room members. This is more efficient than encrypting each message N times for N participants, but provides weaker forward secrecy (a compromised session key decrypts all subsequent messages in that session until rekeying).

Vodozemac uses established cryptographic primitives:

| Operation | Primitive | Library |
|---|---|---|
| Identity keys | Ed25519 | ed25519-dalek |
| Key agreement | X25519 Diffie-Hellman | x25519-dalek |
| Message encryption | AES-256-CTR + HMAC-SHA-256 | aes, sha2 |
| Key derivation | HKDF-SHA-256 | hkdf |
| Message authentication | HMAC-SHA-256 | hmac |

**Audit status:** Vodozemac was independently audited by Least Authority in March 2022, funded jointly by the Matrix.org Foundation and gematik (Germany's national digital health agency). The audit identified 10 findings, 8 of which were resolved during the audit period. The two remaining items (insufficient key zeroization and potential for Olm session creation via one-time key reuse) were addressed in subsequent releases.

Vodozemac benchmarks 5-6x faster than the legacy libolm C library it replaces, while eliminating C memory safety vulnerabilities.

### 2.2 Post-Quantum Cryptography

Quantum computers capable of breaking Curve25519 and RSA do not exist today, but encrypted communications captured now can be stored and decrypted later when such computers become available. This "harvest now, decrypt later" threat is why post-quantum cryptography matters for messaging.

NIST finalized three post-quantum standards in 2024:

- **FIPS 203 (ML-KEM):** Key encapsulation mechanism for key exchange, replacing ECDH
- **FIPS 204 (ML-DSA):** Digital signatures, replacing ECDSA/EdDSA
- **FIPS 205 (SLH-DSA):** Hash-based signatures as a conservative alternative

The messenger landscape has already started adopting PQC:

**Signal** deployed PQXDH (X25519 + ML-KEM-768 hybrid) for initial key exchange in September 2023, and the SPQR ratchet (sparse post-quantum ratchet distributing ML-KEM-768 chunks across message headers) in October 2025. Signal now provides both PQ forward secrecy and PQ post-compromise security.

**SimpleX Chat** integrated sntrup761 (a lattice-based KEM) into every Double Ratchet step in March 2024. Every single message exchange includes a PQ key encapsulation.

**Matrix** has no post-quantum implementation yet. Vodozemac uses only classical Curve25519/Ed25519. The Matrix Foundation has announced PQC spec development but has not published a timeline.

**SimpleGoX will be the first Matrix client with post-quantum protection.** The plan uses ML-KEM-768 in hybrid mode (classical X25519 + ML-KEM-768) via libcrux-ml-kem, a formally verified Rust implementation by Cryspen with AVX2/NEON SIMD optimization. The hybrid approach ensures that even if ML-KEM is found to have weaknesses, the classical X25519 layer provides a safety net. This directly aligns with BSI (German Federal Office for Information Security) TR-02102 guidance, which strongly recommends hybrid PQC since January 2026.

On ARM embedded hardware, ML-KEM-768 key encapsulation takes approximately 0.1ms with NEON acceleration. The bandwidth cost is higher (ML-KEM-768 key + ciphertext = 2,272 bytes vs. 64 bytes for X25519), but this is negligible for messaging workloads.

### 2.3 Encryption at Rest

All data stored by SimpleGoX is encrypted before it reaches the storage medium:

**SQLite databases** (session state, message history, encryption keys) use SQLCipher with AES-256-CBC and a 256-bit key derived from the OS keyring secret via HKDF. The database is configured with `journal_mode = DELETE` (not WAL, which can leak data to journal files), `secure_delete = ON` (overwrite deleted content with zeros), and `temp_store = MEMORY` (prevent temporary data from reaching disk).

**Media files** (images, videos, documents) are encrypted individually with AES-256-GCM using per-file keys derived via HKDF from the database master key plus a random nonce. Encrypted media is stored with the `.sgx` extension; the original filename and MIME type are stored inside the encrypted envelope.

**Configuration files** contain no secrets. All sensitive material (session tokens, encryption keys, server credentials) is stored in the encrypted SQLite database, not in configuration files.

---

## 3. Hardware Security Classes

### 3.1 The Principle: One Codebase, Three Security Levels

The same Tauri application binary runs on all hardware classes. The hardware changes around it, adding layers of physical security, but the application code remains identical. This means a bug fix benefits all classes simultaneously, cryptographic updates deploy universally, and testing covers all variants.

### 3.2 Class 1: SimpleGoX Maker (80-350 EUR)

**Target audience:** Privacy-conscious individuals, makers, developers, small organizations

**Hardware:** Raspberry Pi Zero 2W (4x Cortex-A53 @ 1 GHz, 512 MB RAM) through Raspberry Pi 5 (4x Cortex-A76 @ 2.4 GHz, up to 8 GB RAM) with touchscreen displays.

**Operating system:** Buildroot-generated minimal Linux. Where a standard Raspberry Pi OS Lite installation includes approximately 1,200 packages, the SimpleGoX Class 1 image contains fewer than 50. The root filesystem is a read-only SquashFS image. Runtime writes go to an OverlayFS tmpfs layer that vanishes on reboot.

**What you get over the desktop software:** Dedicated device (no other software running, reduced attack surface), read-only OS (malware cannot persist across reboots), no shell/SSH/package manager (no remote attack vectors), LUKS2 encrypted data partition, boot time under 3 seconds.

**What you do NOT get:** No hardware crypto acceleration, no secure boot chain (Pi bootloader is not cryptographically verified), no tamper detection, no physical security features.

**Delivery:** SD card image available for download with a step-by-step flash guide. Optional pre-assembled kits via online shop.

### 3.3 Class 2: SimpleGoX Secure (500-2,000 EUR)

**Target audience:** Professional environments requiring documented security (medical practices, law firms, financial advisors, SMBs handling sensitive data)

**Hardware:** Custom PCB based on the NXP i.MX 93 SoC or STM32MP257. The NXP i.MX 93 was selected for its EdgeLock Enclave, a dedicated security subsystem with its own processor that operates independently from the main application cores. Keys processed inside the EdgeLock Enclave never leave the enclave boundary. The STM32MP257 is the alternative for designs requiring maximum tamper detection with 12 dedicated tamper pins, SHA-3 hardware support, and DPA-protected cryptographic operations.

**Operating system:** Yocto Linux with a complete verified boot chain from OTP fuses through BootROM, ARM Trusted Firmware, OP-TEE, U-Boot, Linux kernel, and dm-verity verified SquashFS rootfs. Every link in this chain is cryptographically verified. If any single component is tampered with, the device refuses to boot.

**Dual-vendor secure elements:** NXP SE050 (CC EAL6+, FIPS 140-2 Level 3, supports Curve25519/Ed25519 natively) and Infineon OPTIGA Trust M (CC EAL6+, PSA Certified Level 3, used for TLS client certificates and device authentication).

**Security features:** Verified boot chain, hardware-backed key storage, read-only root filesystem with dm-verity, LUKS2 data partition bound to secure boot state, SELinux enforcing mode, kernel lockdown in confidentiality mode, GrapheneOS hardened_malloc, signed OTA updates via RAUC with anti-rollback protection, light sensor tamper detection.

**Optional integrated homeserver:** Tuwunel (Matrix homeserver) runs on the same device, creating a completely self-contained communication system.

### 3.4 Class 3: SimpleGoX Vault (2,000-20,000 EUR)

**Target audience:** Government agencies, military, investigative journalists, human rights organizations, executive protection, anyone facing state-level adversaries

**Hardware:** Custom PCB with maximum security features. Triple-vendor secure elements (NXP SE050, Infineon OPTIGA Trust M, Microchip ATECC608B) from three independent manufacturers. The device master key is split using Shamir's Secret Sharing (2-of-3 threshold) with each share stored in a different secure element.

**Tamper detection:** Analog Devices DS3645 secure supervisor with battery-backed SRAM, sub-100-nanosecond key zeroization on tamper events, 8 external tamper input channels, temperature rate-of-change detection, crystal frequency monitor, and battery-backed operation. PCB security mesh with Time Domain Reflectometry fingerprinting.

**Physical kill switches:** SPDT toggle switches physically sever power to microphone/camera, WiFi/Bluetooth, and cellular modem. Hard-wired indicator LEDs that software cannot fake.

**Duress mode:** A designated PIN triggers immediate key zeroization and data overwrite while appearing to function normally.

**Connectivity options:** WiFi, LoRa (2-15 km mesh), 4G/5G, and satellite (Iridium 9603N) depending on configuration.

---

## 4. Minimal Linux Operating System

### 4.1 Design Principle: The Device is Not a Computer

SimpleGoX hardware devices run a minimal Linux built specifically for one purpose.

**Class 1 (Buildroot):** Root filesystem under 50 MB containing only the Linux kernel, BusyBox, Tauri runtime dependencies, and the SimpleGoX application. No shell in production builds. Boot time under 3 seconds.

**Class 2/3 (Yocto):** More sophisticated build system with long-term maintenance capabilities, recipe-based dependency tracking, and formal SBOM generation. 100-200 MB image.

### 4.2 Kernel Hardening

The Linux kernel is hardened following GrapheneOS and Kernel Self Protection Project practices: SELinux enforcing mode with strict policy, seccomp-BPF syscall filtering (~80 allowed syscalls), Landlock LSM filesystem sandboxing, stack variable zero-initialization, hardened usercopy, slab freelist hardening, KASLR, stack protector, kernel lockdown in confidentiality mode, and GrapheneOS hardened_malloc.

### 4.3 Read-Only Root with Verified Integrity

The root filesystem is a compressed SquashFS image verified block-by-block against a Merkle hash tree using dm-verity. The root hash is embedded in the signed kernel command line. Any single byte modification causes an I/O error. Runtime writes use OverlayFS backed by tmpfs (RAM-only) and vanish on reboot.

### 4.4 Update Mechanism

Over-the-air updates use RAUC with mandatory CMS/X.509 PKI signatures, anti-rollback versioning, A/B partition scheme for automatic rollback, encrypted bundle support, dm-verity-compatible streaming installation, and a 512 KB binary footprint.

---

## 5. Secure Data Deletion

### 5.1 Crypto-Shredding

SimpleGoX encrypts ALL data from the moment of creation. Destroying a single encryption key renders entire data sets permanently inaccessible. Deleting a conversation destroys the per-conversation key. Account wipe destroys the database master key. Device decommissioning triggers secure element key zeroization.

### 5.2 SQLite Secure Deletion

SQLite is configured with `secure_delete = ON`, `journal_mode = DELETE`, `temp_store = MEMORY`, and SQLCipher full-database encryption with AES-256.

### 5.3 RAM Protection

All key material uses the `zeroize` crate with volatile writes and compiler fences. Memory containing keys is locked with mlock(), core dumps are disabled, and long-lived secrets use Ascon128a-encrypted in-memory storage via the `memsecurity` crate.

---

## 6. Comparison with Existing Messengers

| Feature | SimpleGoX | Signal | Element | SimpleX | Threema | Wire |
|---|---|---|---|---|---|---|
| Protocol | Multi (4) | Signal | Matrix | SMP | Ibex | MLS |
| Desktop framework | Tauri (Rust) | Electron | Electron | Qt/Haskell | Native | Electron |
| Crypto runtime | Native Rust | Rust+WASM | Rust+WASM | Haskell | Native | Rust+WASM |
| Post-quantum | Planned (ML-KEM) | Yes (PQXDH+SPQR) | No | Yes (sntrup761) | No | No |
| Embedded Tor | Yes (Arti) | No | No | No | No | No |
| Embedded I2P | Yes (i2pd) | No | No | No | No | No |
| Hardware security | 3 classes | No | No | No | No | No |
| Secure Elements | Triple-vendor | No | No | No | No | No |
| Verified boot | Yes (Class 2/3) | No | No | No | No | No |
| Tamper detection | Yes (Class 2/3) | No | No | No | No | No |
| Kill switches | Yes (Class 3) | No | No | No | No | No |
| Self-hostable | Yes (federation) | No | Yes | Yes (relays) | No | Yes |
| Crypto-shredding | Yes | No | No | No | No | No |
| Open source | Full stack | Client only | Full stack | Full stack | Client only | Full stack |
| Phone required | No | Yes | No | No | No | Yes |

---

## 7. Certification Roadmap

### 7.1 BSI (German Federal Office for Information Security)

SimpleGoX's architecture directly targets VS-NfD approval. The combination of audited Matrix encryption (vodozemac), hardware-backed key storage (certified secure elements), verified boot, and a hardened single-purpose OS meets or exceeds the BSI requirements profile. BSI TR-02102 (January 2026) now recommends ML-KEM, FrodoKEM, and Classic McEliece for key exchange with hybrid mode strongly recommended.

### 7.2 Common Criteria

For a messenger application, EAL4+ is the maximum practical level, requiring 7-24 months and $175K-750K. SimpleGoX inherits EAL6+ from its SE050 and OPTIGA Trust M secure elements.

### 7.3 FIPS 140-3

FIPS 140-2 moves to the Historical List on September 21, 2026. The NXP SE050 holds FIPS 140-2 Level 3 validation.

### 7.4 EU Regulatory Compliance

**GDPR Articles 25 and 32:** SimpleGoX's crypto-shredding approach satisfies the Right to Erasure requirement completely.

**NIS2 Directive (2022/2555):** Requirements include risk management, 24-hour incident reporting, supply chain security, and multi-factor authentication.

**EU Cyber Resilience Act (2024/2847):** Mandates security update capability for all products with digital elements, with full compliance required by December 2027. SimpleGoX's RAUC-based OTA update pipeline directly satisfies this requirement.

---

## 8. Known Limitations and Honest Assessment

No security system is perfect. This section documents what SimpleGoX cannot protect against:

**Rubber hose cryptanalysis:** Physical coercion defeats any technical measure. The duress mode (Class 3) provides partial mitigation.

**Compromised supply chain:** Triple-vendor secure elements and PCB security mesh mitigate but cannot eliminate this risk.

**Zero-day vulnerabilities:** Kernel hardening, seccomp, and SELinux reduce impact but cannot prevent exploitation.

**Metadata on federated protocols:** Matrix federation requires homeservers to exchange metadata in the clear. SimpleX mitigates this with its zero-identifier design.

**Quantum computers (near-term):** Until ML-KEM hybrid mode is deployed, current sessions are vulnerable to "harvest now, decrypt later" attacks.

**Software update trust:** The OTA update mechanism requires trusting the update signing key. Key management follows industry best practices but the risk cannot be fully eliminated.

---

## 9. Modular Replacement Roadmap

### 9.1 Principle: Build on Giants, Then Replace

SimpleGoX Phase 1 uses the best available open-source libraries to reach a functional product quickly. Phase 2 systematically replaces each external dependency with a custom implementation under full project control. The modular architecture ensures that each replacement is independent.

### 9.2 Replacement Schedule

| Component | Phase 1 (Current) | Phase 2 (Planned) | Priority | Difficulty |
|---|---|---|---|---|
| Matrix Client | matrix-rust-sdk | Custom Matrix client in Rust | Medium | High |
| Matrix Crypto | Vodozemac (via SDK) | Custom Olm/Megolm + PQ hybrid | Low | Very High |
| Tor Router | arti-client 0.41 | Custom Tor transport or maintained fork | Low | Very High |
| I2P Router | i2pd 2.56.0 (C++ sidecar) | Native Rust (emissary-core when stable, or custom) | Low | High |
| Telegram Bridge | TDLib via gRPC sidecar | Custom MTProto implementation in Rust | Medium | High |
| SimpleX Client | External dependency | Custom SMP client in Rust (in progress) | High | Medium |
| HTTP Client | reqwest | Custom minimal HTTP client | Low | Medium |
| SOCKS Proxy | Custom bridge | Optimized proxy with traffic analysis resistance | Medium | Medium |
| gRPC Layer | tonic | Direct IPC (Unix sockets or shared memory) | Low | Low |

### 9.3 Replacement Criteria

A dependency is replaced when any of these conditions is met: a security vulnerability is not patched promptly by the upstream maintainer, the dependency's API constraints prevent implementing a required feature, the dependency introduces unwanted transitive dependencies, the project has sufficient resources and expertise to maintain the replacement long-term, or the replacement has been independently reviewed and passes the same test suite.

### 9.4 Custom Cryptography Policy

Cryptographic implementations are the LAST components to be replaced. The industry consensus is clear: do not roll your own crypto unless you have the resources for formal verification and independent audit. SimpleGoX will continue using Vodozemac and established Rust crypto crates until a formal specification is published and reviewed, the implementation is complete with comprehensive test vectors, at least one independent security audit is completed, and the replacement demonstrates equivalent or superior performance.

---

## 10. Quality Assurance

### 10.1 Code Review Protocol

Every file in the SimpleGoX codebase will undergo manual line-by-line review before the first stable release.

**Phase 1 - Structural Review:** For each source file, verify that purpose and responsibility are clear and singular, there is no dead code or untracked TODO items, error handling is explicit (no unwrap() in production paths), all public functions have documentation comments, no hardcoded credentials or secrets exist, and no unsafe blocks appear without documented justification.

**Phase 2 - Security Review:** For each source file, verify that all user input is validated before processing, all cryptographic material uses the zeroize crate, no sensitive data appears in log messages, no TOCTOU race conditions exist, memory-mapped or locked memory is used for key material, and serialization/deserialization cannot trigger arbitrary code execution.

**Phase 3 - Integration Review:** For each module boundary, verify that IPC calls validate all parameters, state transitions are atomic, concurrent access is properly synchronized, and resource cleanup happens on all code paths.

### 10.2 Static Analysis Pipeline

| Tool | Purpose | Configuration |
|---|---|---|
| cargo clippy | Lint for common Rust mistakes | Deny all warnings, pedantic mode |
| cargo audit | Check dependencies for known CVEs | Block build on any advisory |
| cargo deny | License and duplicate dependency checking | AGPL-3.0-or-later/MIT whitelist only |
| cargo fuzz | Fuzz testing for parser and protocol code | Continuous integration |
| cargo tarpaulin | Code coverage measurement | Minimum 80% line coverage target |
| svelte-check | TypeScript/Svelte type checking | Strict mode |

### 10.3 Penetration Testing Protocol

Penetration testing follows a systematic methodology after the code review is complete:

**Phase 1 - Network Analysis:** mitmproxy to intercept all traffic and verify TLS configuration, Wireshark to capture all network interfaces during Tor/I2P operation to detect DNS leaks, and explicit DNS leak testing to verify queries route through the anonymization network.

**Phase 2 - Proxy Verification:** Verify Tor exit IP matches expected exit node, verify I2P traffic uses only .b32.i2p addresses, verify switching between modes does not leak state, and test rapid switching to detect race conditions.

**Phase 3 - Application Security:** OWASP Top 10 adapted for desktop applications, IPC fuzzing with malformed invoke() calls, memory analysis with Valgrind/AddressSanitizer, key material verification by dumping process memory, and clipboard monitoring to verify auto-clear.

**Phase 4 - Protocol-Specific Testing:** Matrix E2E encryption verification, Telegram MTProto session isolation, SimpleX SMP queue isolation, and metadata correlation testing between queues.

**Phase 5 - Reporting:** Each finding is documented with severity, affected component, reproduction steps, recommended fix, and fix verification.

---

## 11. Infrastructure

### 11.1 VPS Configuration

SimpleGoX infrastructure runs on a single VPS (Debian 13) at matrix.simplego.dev hosting multiple services simultaneously:

**Tuwunel 1.5.1** (Matrix Homeserver): Deployed via Docker, accessible on port 8448. Handles Matrix federation with other homeservers.

**SimpleX SMP Server**: Provides SimpleX Messaging Protocol relay on ports 5223 (SMP TLS) and 5224 (SMP Control).

**Nginx Stream Proxy**: Port 443 with ssl_preread distributes TLS connections based on SNI.

**i2pd 2.56.0** (I2P Router): Provides I2P hidden service access to the Matrix homeserver via a server tunnel mapping the .b32.i2p address to localhost:8448. Operates on its own ports (7656 SAM, 4447 SOCKS, 7070 WebUI, 10124 NTCP2/SSU2) without interfering with any other service.

### 11.2 Network Architecture

```
Internet
    |
    +-- Port 443 (nginx ssl_preread)
    |       +-- SNI: matrix.simplego.dev --> Tuwunel :8448
    |       +-- SNI: simplego.dev --> Website
    |
    +-- Port 80 (nginx) --> Website / ACME challenges
    |
    +-- Port 5223 (direct) --> SimpleX SMP Server
    |
    +-- Port 10124 TCP/UDP (i2pd) --> I2P Network
    |
    +-- I2P Hidden Service
            +-- .b32.i2p:8448 --> localhost:8448 --> Tuwunel
```

The key architectural insight: i2pd and Tor hidden services are ADDITIVE. They provide additional network entrances to existing services without modifying those services. Federation continues to work normally over the clearnet while anonymous access is available through the overlay networks.

### 11.3 Firewall Configuration

| Port | Protocol | Service | Description |
|---|---|---|---|
| 22 | TCP | SSH | Server administration |
| 80 | TCP | HTTP | Website, ACME challenges |
| 443 | TCP | HTTPS | Matrix, websites (SNI-routed) |
| 5223 | TCP | SMP TLS | SimpleX SMP server |
| 5224 | TCP | SMP Control | SimpleX control port |
| 8444 | TCP | WebSocket | SimpleX WebSocket (goChatX) |
| 10124 | TCP+UDP | I2P | NTCP2 and SSU2 transport |

---

## 12. Development Workflow

### 12.1 Roles

**Architect (Mausi):** Strategy, architecture decisions, research, security analysis, and authoring detailed technical briefings for the implementation team.

**Implementer (Ritter/Cloudcoat):** Executes briefings locally. No git push or remote commits without explicit approval. All code changes are reviewed before being committed.

**Decision Maker (Sascha):** Tests all changes, provides debug output, makes final decisions on architecture and features. No code is committed without testing and approval.

### 12.2 Briefing System

All implementation work follows a briefing document system. Each briefing contains a clear problem statement, specific files to read before making changes, implementation steps in execution order, success criteria with verifiable checkpoints, and a commit message in Conventional Commits format.

### 12.3 Known Reliability Pattern

The implementation team has a documented tendency to mark work as complete without proper testing. All implementation results require debug output from PowerShell before any fix attempts, verification that the feature actually works (not just compiles), and explicit confirmation of each success criterion. This pattern is documented not as criticism but as a process safeguard that has prevented numerous regressions.

### 12.4 Commit Convention

All commits follow the Conventional Commits format: `type(scope): description`. Types: feat, fix, docs, refactor, test, chore. Scopes: core, tor, i2p, telegram, simplex, ui, scripts. Version numbers are NEVER changed without explicit approval from the decision maker.

---

## References and Further Reading

**Vodozemac audit:** Least Authority, "vodozemac Security Audit Report," March 2022

**Tauri v2 audit:** Radically Open Security, "Penetration Test Report Tauri 2.0," August 2024

**NIST post-quantum standards:** FIPS 203 (ML-KEM), FIPS 204 (ML-DSA), FIPS 205 (SLH-DSA), 2024

**NIST SP 800-88 Rev. 2:** "Guidelines for Media Sanitization," September 2025

**BSI TR-02102:** "Cryptographic Mechanisms: Recommendations and Key Lengths," January 2026

**Signal PQXDH:** Signal blog, "Quantum Resistance and the Signal Protocol," September 2023

**SimpleX PQ Double Ratchet:** simplex-chat RFC, "Post-Quantum Double Ratchet," 2023

**Matrix cryptographic analysis:** ETH Zurich, "The Matrix Reloaded: A Mechanized Formal Analysis of the Matrix Cryptographic Suite," 2024

**Arti:** The Tor Project, "Arti: A pure-Rust Tor implementation"

**emissary:** Aaro Altonen, "Rust implementation of the I2P protocol stack," github.com/eepnet/emissary (tested v0.4.0, three bugs discovered and reported)

**i2pd:** PurpleI2P, "Full-featured C++ I2P implementation," github.com/PurpleI2P/i2pd (stable since 2016, used as production sidecar)

**I2P Project:** "New I2P Routers," geti2p.net/en/blog/post/2025/10/16/new-i2p-routers

**NXP SE050 datasheet:** Rev. 3.8, October 2023

**DS3645 tamper supervisor:** Analog Devices/Maxim Integrated datasheet

---

*SimpleGoX, IT and MORE Systems, Recklinghausen*
*Secure communication from silicon to screen*
