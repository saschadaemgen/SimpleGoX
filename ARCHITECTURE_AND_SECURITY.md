# SimpleGoX - Architecture & Security

**Document version:** April 2026
**Project:** SimpleGoX Multi-Messenger Platform
**License:** Apache-2.0
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

The same Tauri application binary runs on all hardware classes. The hardware changes around it, adding layers of physical security, but the application code remains identical. This means:

- A bug fix benefits all classes simultaneously
- Cryptographic updates deploy universally
- Testing covers all variants

### 3.2 Class 1: SimpleGoX Maker (80-350 EUR)

**Target audience:** Privacy-conscious individuals, makers, developers, small organizations

**Hardware:** Raspberry Pi Zero 2W (4x Cortex-A53 @ 1 GHz, 512 MB RAM) through Raspberry Pi 5 (4x Cortex-A76 @ 2.4 GHz, up to 8 GB RAM) with touchscreen displays.

**Operating system:** Buildroot-generated minimal Linux. Where a standard Raspberry Pi OS Lite installation includes approximately 1,200 packages, the SimpleGoX Class 1 image contains fewer than 50. The root filesystem is a read-only SquashFS image. Runtime writes go to an OverlayFS tmpfs layer that vanishes on reboot.

**What you get over the desktop software:**

- Dedicated device (no other software running, reduced attack surface)
- Read-only OS (malware cannot persist across reboots)
- No shell, no SSH, no package manager (no remote attack vectors)
- LUKS2 encrypted data partition
- Boot time under 3 seconds

**What you do NOT get:**

- No hardware crypto acceleration (software-only encryption)
- No secure boot chain (Pi bootloader is not cryptographically verified)
- No tamper detection
- No physical security features

**Delivery:** SD card image available for download with a step-by-step flash guide. Optional pre-assembled kits via online shop.

### 3.3 Class 2: SimpleGoX Secure (500-2,000 EUR)

**Target audience:** Professional environments requiring documented security (medical practices, law firms, financial advisors, SMBs handling sensitive data)

**Hardware:** Custom PCB based on the NXP i.MX 93 SoC or STM32MP257.

The **NXP i.MX 93** was selected for its EdgeLock Enclave, a dedicated security subsystem with its own processor that operates independently from the main application cores. Keys processed inside the EdgeLock Enclave never leave the enclave boundary. The enclave handles secure boot verification, random number generation, key storage, and cryptographic operations in hardware. This is architecturally similar to Apple's Secure Enclave, but available for custom embedded designs at $8-14 per unit.

The **STM32MP257** is the alternative for designs requiring maximum tamper detection. It provides 12 dedicated tamper pins (5 active, 7 passive), on-chip temperature/voltage/frequency monitors, SHA-3 hardware support, DPA-protected cryptographic operations, and targets SESIP3 certification.

**Operating system:** Yocto Linux with a complete verified boot chain:

```
OTP Fuses (irreversible, burned once)
    |
    +-- Store root-of-trust public key hash
    |
BootROM (silicon-embedded, immutable)
    |
    +-- Verify ARM Trusted Firmware (TF-A BL2) signature
    |
TF-A BL2 (verified by BootROM)
    |
    +-- Verify OP-TEE (secure world) + U-Boot (normal world)
    |
OP-TEE (ARM TrustZone secure world)
    |
    +-- PKCS#11 Trusted Application (software HSM)
    +-- Secure key storage in eMMC RPMB
    |
U-Boot (verified by TF-A)
    |
    +-- Verify Linux kernel FIT image signature
    |
Linux Kernel (verified by U-Boot)
    |
    +-- dm-verity: verify SquashFS rootfs block-by-block
    |
Application (launches as PID 1, verified rootfs)
```

Every link in this chain is cryptographically verified. If any single component is tampered with, the device refuses to boot. Closing the security fuses (setting SEC_CONFIG) is irreversible. Once closed, the chip permanently rejects unsigned images.

**Dual-vendor secure elements:**

- **NXP SE050** (primary): CC EAL6+ and FIPS 140-2 Level 3 certified. Supports Curve25519/Ed25519 natively in hardware, enabling direct Matrix identity key operations without software fallback.
- **Infineon OPTIGA Trust M** (secondary): CC EAL6+ and PSA Certified Level 3. Supports NIST curves and RSA, used for TLS client certificates and device authentication.

**Security features:**

- Verified boot chain from silicon fuses to application
- Hardware-backed key storage (keys never exist in main memory)
- Read-only root filesystem with dm-verity integrity verification
- LUKS2 data partition with key bound to secure boot state (tampered boot = no data access)
- SELinux in enforcing mode with strict policy
- Kernel lockdown in confidentiality mode
- GrapheneOS hardened_malloc (guard pages, slab canaries, randomization)
- Signed OTA updates via RAUC with anti-rollback protection
- Light sensor tamper detection (detects enclosure opening)

**Optional integrated homeserver:** Tuwunel (Matrix homeserver) runs on the same device, creating a completely self-contained communication system that requires no external server.

### 3.4 Class 3: SimpleGoX Vault (2,000-20,000 EUR)

**Target audience:** Government agencies, military, investigative journalists, human rights organizations, executive protection, anyone facing state-level adversaries

**Hardware:** Custom PCB with maximum security features. The BOM cost ranges from $800 (base) to $7,000+ (full specification), with the retail price reflecting engineering, certification, and support costs.

**SoC selection for Class 3:**

The NXP i.MX 93 remains the primary recommendation for its EdgeLock Enclave. For configurations requiring maximum computational performance (video processing, multiple simultaneous encrypted streams), the NXP i.MX 8M Plus (4x Cortex-A53 @ 1.8 GHz + NPU) provides additional headroom with CAAM (Cryptographic Acceleration and Assurance Module) hardware crypto.

**Triple-vendor secure elements:**

Three secure elements from three independent manufacturers ensure that a compromise, backdoor, or vulnerability in any single vendor's silicon cannot expose the device master key:

| Secure Element | Manufacturer | Certification | Key Capabilities |
|---|---|---|---|
| SE050E | NXP (Netherlands) | CC EAL6+, FIPS 140-2 L3 | Curve25519, Ed25519, NIST curves, RSA-4096, AES-256-GCM |
| OPTIGA Trust M | Infineon (Germany) | CC EAL6+, PSA L3 | NIST P-256/P-384/P-521, Brainpool, RSA-2048, AES-256 |
| ATECC608B | Microchip (USA) | Secure Key Storage | NIST P-256, SHA-256, AES-128 |

The device master key is split using **Shamir's Secret Sharing (2-of-3 threshold)**:

1. The SE050 generates a 256-bit master key internally (the raw key never leaves the SE050)
2. A Shamir polynomial splits the key into three shares
3. Each share is stored in a different secure element
4. At runtime, any two of three shares reconstruct the key inside TrustZone secure world
5. The reconstructed key is immediately used to derive session keys, then zeroized

This means:
- Compromising any one chip reveals nothing (a single share is mathematically useless)
- The device continues to function even if one secure element fails
- Three different supply chains, three different countries, three different silicon designs

**Tamper detection and response:**

The **Analog Devices DS3645** secure supervisor is the cornerstone of physical security. It provides:

- 4 KB of battery-backed SRAM with constant complementing (bit patterns flip continuously, making cold boot attacks impossible)
- Hardwired zeroization of all stored secrets in under 100 nanoseconds on any tamper event
- 8 external tamper input channels (mesh, light sensors, mechanical switches)
- Internal temperature sensor with rate-of-change detection (defeats thermal attacks where an attacker cools the device to slow DRAM decay)
- Crystal frequency monitor (detects clock manipulation)
- Battery-backed operation (tamper vigilance continues when the device is unpowered)

**PCB security mesh:** Serpentine copper traces on inner PCB layers are continuously monitored for opens, shorts, and impedance changes. Advanced implementations use Time Domain Reflectometry (TDR) to create a unique analog fingerprint of the mesh. Any physical modification (drilling through the PCB, soldering probe wires, removing components) changes the TDR fingerprint and triggers immediate key zeroization.

**Physical kill switches:** Following the design pioneered by Purism's Librem 5, three SPDT toggle switches physically sever the power rail to:

1. Microphone and camera
2. WiFi and Bluetooth module
3. Cellular modem

Each switch includes a hard-wired indicator LED in series with the component's power line. When the switch is off, the LED is physically disconnected from power. Software cannot fake the LED state. Each switched component occupies its own isolated power domain, preventing the SoC's power management from re-enabling disabled peripherals.

**Potted enclosure:** The electronics are encased in epoxy resin within a CNC-machined aluminum housing. Physical access to the PCB requires destroying the enclosure, which triggers multiple tamper sensors simultaneously. The aluminum housing also functions as a Faraday cage with RF gaskets at all seams.

**Duress mode ("Brick Me" PIN):** If the user enters a designated duress PIN instead of their real PIN, the device appears to function normally while simultaneously triggering immediate key zeroization, overwriting the encrypted data partition with random data, and transmitting a silent distress signal (if network-connected). The attacker sees a functioning device with no usable data.

**Air-gap mode:** For environments where no wireless emissions are permitted, all radios can be disabled via kill switches and communication happens through QR codes displayed on screen and scanned by another SimpleGoX device. This enables secure message exchange with zero electronic emissions.

**Connectivity options:**

| Configuration | Modules | Purpose |
|---|---|---|
| WiFi only | Integrated | Standard operation |
| WiFi + LoRa | SX1262 (868/915 MHz) | Long-range mesh (2-15 km) |
| WiFi + 4G | EG25-G | Always-on cellular |
| WiFi + LoRa + 5G | Both | Full connectivity |
| WiFi + LoRa + Satellite | Iridium 9603N | Last-resort global coverage |

**LoRaWAN gateway mode:** Class 3 devices equipped with the SX1302 8-channel gateway concentrator can act as communication gateways for other SimpleGoX devices within a 2-15 km radius. Messages from LoRa-connected devices are bridged to Matrix over the gateway's Internet connection, enabling mesh communication in infrastructure-denied environments.

**Optional: Embedded HSM:** For maximum cryptographic assurance, a YubiHSM 2 (USB nano form factor, FIPS 140-2 Level 3 validated) can be integrated directly onto the PCB. This provides an additional hardware-protected key store with its own audit-logged key management and 16 sessions for concurrent cryptographic operations. At approximately $650, it is reserved for the highest-tier configurations.

---

## 4. Minimal Linux Operating System

### 4.1 Design Principle: The Device is Not a Computer

A standard Linux distribution (Debian, Ubuntu, Fedora) includes thousands of packages, services, and utilities designed for general-purpose computing. Each package is a potential attack surface. A web server, a package manager, an SSH daemon, a mail client - none of these belong on a dedicated communication device.

SimpleGoX hardware devices run a minimal Linux built specifically for one purpose:

**Class 1 (Buildroot):** Produces a root filesystem under 50 MB containing only the Linux kernel, BusyBox (minimal userspace), Tauri runtime dependencies, and the SimpleGoX application. No shell is available in production builds. The entire system boots in under 3 seconds.

**Class 2/3 (Yocto):** Provides a more sophisticated build system with long-term maintenance capabilities, recipe-based dependency tracking, and formal SBOM (Software Bill of Materials) generation for compliance requirements. The resulting image is larger (100-200 MB) but still orders of magnitude smaller than a desktop distribution.

### 4.2 Kernel Hardening

The Linux kernel is configured and hardened following practices established by GrapheneOS and the Kernel Self Protection Project:

**Mandatory Access Control:** SELinux runs in enforcing mode with a custom strict policy. Every process, file, and network socket has a security context. No process runs in the permissive `unconfined` domain. The SimpleGoX application runs in a dedicated `sgx_app_t` domain with access only to its own data directories, the display server, and network sockets.

**Syscall filtering:** seccomp-BPF profiles restrict each process to the minimum set of system calls required for operation. The Tauri application needs approximately 80 syscalls; the remaining 300+ are blocked. Any attempt to call a blocked syscall terminates the process immediately.

**Filesystem sandboxing:** Landlock LSM (available since Linux 5.13, enhanced in 6.x) provides unprivileged filesystem access control that stacks with SELinux. Each protocol handler can only access its own data directory.

**Memory hardening:**

- `CONFIG_INIT_STACK_ALL_ZERO`: Initialize all stack variables to zero (prevents information leaks from uninitialized memory)
- `CONFIG_HARDENED_USERCOPY`: Validate all userspace memory copies
- `CONFIG_SLAB_FREELIST_HARDENED`: Protect slab allocator freelists against corruption
- `CONFIG_RANDOMIZE_BASE` (KASLR): Randomize kernel memory layout on every boot
- `CONFIG_STACKPROTECTOR_STRONG`: Canary-based stack buffer overflow detection
- GrapheneOS hardened_malloc as the userspace memory allocator

**Kernel lockdown:** In `confidentiality` mode, the kernel prevents reading kernel memory (/dev/mem, /proc/kcore), loading unsigned modules, accessing MSRs, and using kprobes. This prevents a compromised userspace process from extracting kernel secrets.

### 4.3 Read-Only Root with Verified Integrity

The root filesystem is stored as a compressed SquashFS image, which is inherently read-only. Before mounting, the kernel verifies every 4 KB block against a Merkle hash tree using dm-verity. The root hash of this tree is embedded in the kernel command line, which is itself signed and verified by the secure boot chain.

If a single byte of the root filesystem is modified (on disk, in transit, or in memory), dm-verity returns an I/O error and the system refuses to proceed. There is no way to silently modify the OS.

Runtime writes for volatile data (/var, /tmp, /etc modifications) use an OverlayFS layer backed by tmpfs (RAM-only). All changes vanish on reboot. Persistent data (encrypted message database, key material) resides on a separate LUKS2-encrypted partition.

### 4.4 Update Mechanism

Over-the-air updates use RAUC, an open-source update framework with:

- Mandatory CMS/X.509 PKI signatures on all update bundles
- Anti-rollback versioning (prevents downgrade attacks)
- A/B partition scheme (failed update = automatic rollback to last known good image)
- Encrypted bundle support for firmware confidentiality
- dm-verity-compatible streaming installation
- 512 KB binary footprint

RAUC is used in production by Valve (SteamOS) and numerous industrial IoT deployments.

---

## 5. Secure Data Deletion

### 5.1 Why Traditional Deletion Fails

When you "delete" a file on any modern operating system, the file's directory entry is removed but the data remains on the storage medium until overwritten by new data. On traditional hard drives, overwriting with random data was sufficient. On flash storage (SSDs, eMMC, SD cards, USB drives), it is not.

Flash storage uses a Flash Translation Layer (FTL) that maps logical addresses to physical NAND cells. When you write to a "deleted" sector, the FTL may write to a completely different physical cell while the original data persists in the old cell. Additionally, SSDs maintain 7-28% over-provisioned NAND that is invisible to the operating system and cannot be addressed by software writes.

NIST SP 800-88 was revised to Revision 2 in September 2025, explicitly acknowledging this reality. The revision shifts focus from prescriptive overwrite patterns to organizational sanitization programs and defers device-specific techniques to IEEE 2883-2022. The Gutmann 35-pass overwrite method is irrelevant for modern media. Peter Gutmann himself has stated that applying all 35 patterns is pointless since it targets encoding technologies from 30+ year-old magnetic media.

### 5.2 Crypto-Shredding: SimpleGoX's Primary Approach

SimpleGoX encrypts ALL data from the moment of creation. The encryption key hierarchy ensures that destroying a single key renders entire data sets permanently inaccessible:

```
Hardware Root Key (in Secure Element or OS keyring)
    |
    +-- Database Master Key (HKDF derived)
    |       |
    |       +-- Per-conversation keys
    |       +-- Per-media-file keys  
    |       +-- Per-protocol keys
    |
    +-- Temporary Session Keys (in RAM only)
```

**Deleting a conversation:** The per-conversation encryption key is destroyed. The encrypted conversation data remains on disk but is indistinguishable from random noise. No amount of forensic analysis can recover the plaintext without the key.

**Deleting all data (account wipe):** The database master key is destroyed. Every encrypted database, every media file, every cached message becomes permanently inaccessible. This takes milliseconds regardless of how much data exists.

**Device decommissioning:** On Class 2/3 devices, the secure element zeroizes all stored keys on command. The DS3645 tamper supervisor can trigger sub-microsecond zeroization of its battery-backed SRAM. NVMe/eMMC Sanitize commands reset all NAND cells including over-provisioned areas.

NIST SP 800-88 Rev. 2 classifies Cryptographic Erase as Purge-level sanitization, which protects against state-of-the-art laboratory recovery techniques.

### 5.3 SQLite Secure Deletion

Matrix SDK stores message history, room state, and encryption keys in SQLite databases. SimpleGoX configures SQLite with:

- `PRAGMA secure_delete = ON` (overwrite deleted content with zeros)
- `PRAGMA journal_mode = DELETE` (not WAL, which can leak data to journal files)
- `PRAGMA temp_store = MEMORY` (prevent temporary data from reaching disk)
- SQLCipher full-database encryption with AES-256

For conversation deletion, destroying the per-conversation encryption key is instantaneous and reliable, making the database content irrecoverable even if the SQLite file is forensically examined.

### 5.4 RAM Protection

DRAM retains data for seconds to minutes after power loss. The 2008 Princeton cold boot attack demonstrated that BitLocker, FileVault, and LUKS encryption keys could be extracted from RAM using compressed air cooling to slow data decay.

SimpleGoX protects against this through:

**Active zeroization:** All key material in Rust uses the `zeroize` crate, which writes zeros via `core::ptr::write_volatile` followed by a `compiler_fence` to prevent the compiler from optimizing away the write. When a key goes out of scope, it is guaranteed to be zeroed in memory.

**Memory locking:** `mlock()` prevents key-containing memory pages from being written to swap. `prctl(PR_SET_DUMPABLE, 0)` and `setrlimit(RLIMIT_CORE, 0)` prevent core dumps from capturing key material.

**Encrypted in-memory storage:** For long-lived secrets that must remain in RAM (session keys, identity keys), the `memsecurity` crate provides Ascon128a-encrypted storage inspired by OpenSSH's key protection approach.

**On Class 2/3 devices:** External PSRAM (if present) is encrypted before writes using keys held only in internal SRAM or CPU registers.

---

## 6. Comparison with Existing Messengers

### 6.1 Signal

Signal sets the gold standard for protocol security. The Double Ratchet algorithm provides forward secrecy and post-compromise security. PQXDH adds post-quantum protection to initial key exchange. The protocol has been formally verified and extensively audited.

However, Signal Desktop is an Electron application with the structural weaknesses described in Section 1.1. Signal requires a phone number for registration, which is a metadata leak. Signal's server is centralized (operated by the Signal Foundation) with no self-hosting option. Signal provides no hardware security.

SimpleGoX uses vodozemac (same underlying cryptographic design as Signal for Matrix chats, independently audited), runs on Tauri (not Electron), requires no phone number for Matrix accounts, supports federation (self-hosted servers), and adds hardware security layers not available in Signal at any price.

### 6.2 Element Desktop

Element is the reference Matrix client, also built on matrix-rust-sdk and vodozemac. Element Desktop uses Electron. Element Web runs entirely in the browser, where encryption keys exist in JavaScript memory accessible to browser extensions and XSS attacks.

SimpleGoX runs the same audited cryptographic library but in a Tauri context where keys exist in native Rust memory outside the WebView. This is a structural security improvement that Element Desktop cannot match without migrating away from Electron.

### 6.3 SimpleX Chat

SimpleX provides the strongest metadata protection of any messenger. Users have no identifiers of any kind - no phone number, no username, no public key. The SMP protocol uses per-queue ephemeral keys, making traffic correlation between queues computationally infeasible. SimpleX has already integrated post-quantum encryption (sntrup761) into every ratchet step.

SimpleX's limitation is ecosystem size (approximately 300,000 users vs. Matrix's 115+ million) and the absence of federation compatibility with other protocols. SimpleGoX integrates SimpleX as one of four supported protocols, allowing users to benefit from SimpleX's metadata protection while maintaining access to the broader Matrix federation.

### 6.4 Threema

Threema is a Swiss-developed messenger with strong privacy credentials and no requirement for phone number or email registration. However, ETH Zurich researchers discovered seven attacks against Threema's custom cryptographic protocol in 2023, including message reordering, replay, and reflection attacks. Threema subsequently developed the new Ibex protocol to address these findings. The Threema server remains proprietary.

### 6.5 Wire

Wire was the first messenger to deploy MLS (Messaging Layer Security, IETF standard) in production. Wire holds BSI VS-NfD approval (BSI-VSA-10519) for German government classified communications. However, ETH Zurich's 2024 analysis of Wire's MLS implementation found multiple serious vulnerabilities, including trivial message replay and man-in-the-middle attacks.

### 6.6 Comparison Matrix

| Feature | SimpleGoX | Signal | Element | SimpleX | Threema | Wire |
|---|---|---|---|---|---|---|
| Protocol | Multi (4) | Signal | Matrix | SMP | Ibex | MLS |
| Desktop framework | Tauri (Rust) | Electron | Electron | Qt/Haskell | Native | Electron |
| Crypto runtime | Native Rust | Rust+WASM | Rust+WASM | Haskell | Native | Rust+WASM |
| Post-quantum | Planned (ML-KEM) | Yes (PQXDH+SPQR) | No | Yes (sntrup761) | No | No |
| Hardware security | 3 classes | No | No | No | No | No |
| Secure Elements | Triple-vendor | No | No | No | No | No |
| Verified boot | Yes (Class 2/3) | No | No | No | No | No |
| Tamper detection | Yes (Class 2/3) | No | No | No | No | No |
| Kill switches | Yes (Class 3) | No | No | No | No | No |
| Self-hostable | Yes (federation) | No | Yes | Yes (relays) | No | Yes |
| Crypto-shredding | Yes | No | No | No | No | No |
| Open source | Full stack | Client only | Full stack | Full stack | Client only | Full stack |
| Phone required | No | Yes | No | No | No | Yes |
| BSI VS-NfD target | Yes | No | No | No | No | Yes |

---

## 7. Certification Roadmap

### 7.1 BSI (German Federal Office for Information Security)

The BSI publishes a Secure Messenger Requirements Profile (BSI-CI-RP-0024) defining standards for products handling VS-NfD (Verschlusssache - Nur fuer den Dienstgebrauch) classified data. Wire Enterprise already holds VS-NfD approval and is used by 30+ German federal ministries. The German military's BwMessenger is based on the Matrix protocol.

BSI TR-02102 (version January 2026) now recommends ML-KEM, FrodoKEM, and Classic McEliece for key exchange, and ML-DSA and SLH-DSA for digital signatures, with hybrid mode (PQC + classical) strongly recommended. The BSI and 17 European partner agencies call for transition to PQC by 2030.

SimpleGoX's architecture directly targets VS-NfD approval. The combination of audited Matrix encryption (vodozemac), hardware-backed key storage (certified secure elements), verified boot, and a hardened single-purpose OS meets or exceeds the BSI requirements profile.

### 7.2 Common Criteria

Common Criteria (ISO/IEC 15408) defines Evaluation Assurance Levels (EAL) from 1 to 7:

- **EAL1-2:** Functionally tested, structurally tested
- **EAL3:** Methodically tested and checked
- **EAL4:** Methodically designed, tested, and reviewed (includes source code review)
- **EAL5-6:** Semiformally/formally designed and tested (typical for secure elements)
- **EAL7:** Formally verified design and tested (extremely rare, reserved for the most critical systems)

For a messenger application, EAL4+ (EAL4 augmented with vulnerability analysis) is the maximum practical level, requiring 7-24 months and $175K-750K. SimpleGoX inherits EAL6+ from its SE050 and OPTIGA Trust M secure elements for the hardware trust anchors.

### 7.3 FIPS 140-3

FIPS 140-3 (aligned with ISO/IEC 19790) validates cryptographic modules at four security levels. FIPS 140-2 moves to the Historical List on September 21, 2026. The NXP SE050 holds FIPS 140-2 Level 3 validation. For software cryptographic modules, wolfCrypt holds FIPS 140-3 certificates #4718 and #5041 (valid through July 2030), covering both software and hardware-accelerated implementations.

### 7.4 EU Regulatory Compliance

**GDPR Articles 25 and 32** require "data protection by design and by default" and "state of the art" security measures. End-to-end encryption is recognized by the EDPB as the primary technical measure for securing personal data. Article 17 (Right to Erasure) requires complete deletion including backups. SimpleGoX's crypto-shredding approach satisfies this requirement completely.

**NIS2 Directive** (2022/2555) applies to providers of public electronic communications services. Requirements include risk management, 24-hour incident reporting, supply chain security, and multi-factor authentication. Penalties reach 10 million EUR or 2% of global revenue.

**EU Cyber Resilience Act** (2024/2847) mandates security update capability for all products with digital elements, with full compliance required by December 2027. SimpleGoX's RAUC-based OTA update pipeline with mandatory signing directly satisfies this requirement.

---

## 8. Known Limitations and Honest Assessment

No security system is perfect. This section documents what SimpleGoX cannot protect against:

**Rubber hose cryptanalysis:** If an adversary can physically coerce the user into revealing their PIN or password, no technical measure prevents access. The duress mode (Class 3) provides partial mitigation by silently destroying data while appearing to cooperate.

**Compromised supply chain:** If a state-level adversary compromises the hardware manufacturing pipeline before the device reaches the user, they could implant undetectable modifications. The triple-vendor secure element approach and PCB security mesh mitigate but cannot eliminate this risk.

**Zero-day vulnerabilities:** Unknown vulnerabilities in the Linux kernel, Tauri, vodozemac, or any dependency could be exploited before patches are available. Kernel hardening, seccomp, and SELinux reduce the impact but cannot prevent exploitation.

**Metadata on federated protocols:** Matrix federation requires homeservers to exchange metadata (room membership, message timestamps, sender identifiers) in the clear between servers. A compromised homeserver operator can observe who communicates with whom and when, even though message content is encrypted. SimpleX mitigates this with its zero-identifier design, which is why SimpleGoX supports both protocols.

**Quantum computers (near-term):** Until ML-KEM hybrid mode is deployed, current Matrix E2E sessions are vulnerable to "harvest now, decrypt later" attacks. This is a shared vulnerability with Element and every other Matrix client.

**Software update trust:** The OTA update mechanism requires trusting the update signing key. If this key is compromised, malicious updates could be pushed to all devices. Key management follows industry best practices (HSM-stored signing key, air-gapped signing infrastructure), but the risk cannot be fully eliminated.

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

**NXP SE050 datasheet:** Rev. 3.8, October 2023

**DS3645 tamper supervisor:** Analog Devices/Maxim Integrated datasheet

---

*SimpleGoX, IT and MORE Systems, Recklinghausen*
*Secure communication from silicon to screen*
