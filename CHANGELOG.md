# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added - Season 5

#### SimpleX Protocol - Full Bidirectional Chat

##### Decrypt Pipeline (Receive Path)
- Layer 3 decrypt: NaCl crypto_box with queue-level rcvDh keys and padded rcvMeta
- Layer 2 decrypt: per-queue ephemeral NaCl with PubHeader parser (both 72B first-message and 27B regular-message variants)
- AgentConfirmation parser: 'C' tag, X448 SPKI (68 bytes, OID 2b656f), SNTRUP761 KEM slot detection ('P' present, 'A' absent)
- ConnInfo envelope parser: JSON with ConnInfoEnvelope and PeerProfile structure
- ACK dispatch after successful decrypt

##### Key Agreement and Ratchet (Bob Path)
- X3DH key agreement: X448 with HKDF-SHA512, 112-byte AssocData, three Diffie-Hellman operations
- Custom AES-256-GCM with 16-byte IV: manual NIST SP 800-38D Algorithm 4 implementation with GHASH J_0 derivation (standard aes-gcm crate only supports 12-byte nonces)
- Bob Double Ratchet: header decrypt, DH ratchet step, body decrypt, sending-side ratchet rotation
- Chain KDF with salt="" info="SimpleXChainRatchet" producing 96 bytes for (ck', mk, iv1, iv2)
- Root KDF with salt=rcRK info="SimpleXRootRatchet"
- Bob Ratchet state: rcHKr for SameRatchet vs AdvanceRatchet detection, last_snd_msg_hash, next_snd_msg_id

##### Send Path (HELLO + Initial Messages)
- encode_msg_header, chain_kdf, encrypt_message_header, encrypt_and_assemble_ratchet_message
- AgentMessage wire format: 'M' tag, Int64 BE sndMsgId, length-prefixed prevMsgHash, content tag, body
- HELLO 11 bytes -> EncRatchetMessage 15864 bytes -> Layer 2 encrypted 15992 bytes -> SEND "Ok"
- prevMsgHash=&[] encoded as Haskell ByteString (0 length prefix)
- padded_msg_len=13500 for regular HELLO, 13488 for PHConfirmation overhead

##### Desktop Accept Flow
- Outer envelope corrected from 'M' to 'C' (AgentConfirmation)
- Inner content corrected from 'H' (HELLO) to 'I' + profile JSON (AgentConnInfo)
- ClientMessage header corrected from '_' (PHEmpty) to 'K' + 44B senderAuthKey SPKI (PHConfirmation)
- e2eEncryption_ Maybe marker '0' Nothing added
- Invitation URLs include &q=m parameter for mode detection
- agentVersion=1 for chat messages, agentVersion=7 for AgentConfirmation
- peer_e2e_pub persistence for Layer 2 Maybe-Nothing decrypt path
- sender auth keypair persistence per contact

##### Profile System
- Singleton profile table in queue_store SQLite database
- SetProfile and GetProfile gRPC endpoints
- Startup loads profile from disk and logs it
- AgentConnInfo carries real profile (displayName, fullName, bio) instead of placeholders
- Desktop verified: contact appears with "Sascha / Prinz Sascha" display

##### Tauri Integration
- sgx-simplex auto-spawns as Tauri sidecar on port 50053
- CLI args: --port, --data-dir (configurable, defaults preserved for standalone use)
- Registered in tauri.conf.json externalBin alongside sgx-telegram
- Shutdown cleanup via taskkill/killall on app close

##### Realtime Event Stream
- gRPC server-side streaming StreamSimplexUpdates
- tokio::sync::broadcast channel with 256-slot buffer
- Update variants: ContactEstablished, NewMessage, ContactUpdated, HandshakeProgress
- Tauri backend consumes stream and emits sx-contact-established, sx-new-message, sx-contact-updated, sx-handshake-progress events
- Frontend listens via @tauri-apps/api/event

##### Rich Security-Themed Handshake Progress
- Around 18 distinct stages emitted during contact handshake
- Each stage names the cryptographic primitive engaged (TLS 1.3, SHA-256, NaCl crypto_box, X25519, X448 X3DH, SNTRUP761 KEM slot, HKDF-SHA512, Double Ratchet, AES-256-GCM 16-byte IV)
- emit_progress helper broadcasts HandshakeProgress events at each stage
- Messages serve dual purpose: UX feedback plus live documentation of security stack

##### Frontend Integration
- SimpleX-specific Svelte stores (simplexProfile, simplexContacts, simplexMessages)
- Event listeners initialized alongside sx_subscribe_updates Tauri command
- Sidebar merges SimpleX contacts with Matrix rooms and Telegram chats, sorted by last activity
- SX protocol badge on SimpleX contacts alongside MX and TG badges
- ChatView renders SimpleX contact with sx: prefix routing
- Live message reception without refresh (peer messages appear instantly)

##### One-Click Add-Contact UX
- @tauri-apps/plugin-clipboard-manager dependency added
- Add-contact flow: single button click reads clipboard, validates as SimpleX URL (simplex:/ or https://simplex.chat/), dispatches to sx_submit_invitation immediately
- No dialog, no confirmation step
- Toast notification for invalid clipboard content
- Button conditional rendering: only visible when simplexProfile is set

##### StatusBanner Integration
- sx-status events in StatusBanner-compatible format {state, detail}
- Auto-hide after 3 seconds on "connected" state (post identity_established)
- Expandable terminal-style log with timestamps and color coding
- Uses same visual language as tor-status and i2p-status banners

##### SimpleX Disconnect
- ResetSimplex gRPC endpoint clears profile, contacts, and related tables
- sx_disconnect Tauri command with confirmation dialog
- AccountsTab SimpleX card: Disconnect button (replaces Edit Profile, which moves to main frontend in future)
- Set up button appears when profile is empty
- Disconnect clears simplexProfile and simplexContacts stores, hides add-contact button

##### Toast Notification System
- Lightweight Svelte store plus Toast.svelte component
- Right-aligned toast stack with auto-dismiss after 4 seconds
- Level support (info, warn, error, success) with appropriate styling

#### Infrastructure
- Increased broadcast channel buffer to 256 slots to handle rapid progress events

### Changed - Season 5

- Protocol table in README: SimpleX status updated from "In Development (pre-alpha) - queue creation working, AgentConfirmation received" to reflect full bidirectional chat with receive path complete and send path in development
- SimpleX sidecar no longer hardcodes port and data directory; accepts --port and --data-dir CLI arguments
- Account card UI for SimpleX: Edit Profile button removed from Accounts settings (will return in main frontend as profile menu)

### Fixed - Season 5

- Contact handshake against unmodified SimpleX Desktop now succeeds end-to-end (previously rejected with SEInvitationNotFound due to five structural defects in AgentConfirmation encoding)
- prevMsgHash encoding fixed to match Haskell ByteString format (empty hash is zero-length, not absent)
- padded_msg_len adjusted to 13488 for PHConfirmation messages to accommodate 46-byte header overhead
- Peer e2e public key now persisted in queue_store so Layer 2 Maybe-Nothing decrypt path finds the key on subsequent messages

### Added - Season 4

#### SimpleX Protocol Sidecar (sgx-simplex)
- New crate `crates/sgx-simplex/` - native Rust SMP v9 client implementation
- SMP v9 handshake: ClientHello with X25519 session auth key (SPKI 44B), ServerHello parsing with X25519 OID scan for server session public key extraction
- CbAuthenticator (80 bytes): SHA-512 over `[sessIdLen][sessId][corrIdLen][corrId][entityIdLen][entityId][cmd]`, encrypted via NaCl crypto_box (X25519 DH + HSalsa20 + XSalsa20-Poly1305), corrId as nonce
- Separate `queue_auth_keypair` (X25519) per queue, distinct from session auth keypair
- NEW command v9: queue_auth_public first (rcvAuthKey), rcv_dh_public second (rcvDhKey), `0ST` suffix (basicAuth Nothing + Subscribe + sndSecure True)
- Response parser v9: corrId 24B random, no session_id on wire (implySessId=true)
- IDS response parser: real rcv_id and snd_id extracted with length-prefixed parsing
- TLS fingerprint verification for SMP server identity (SHA-256 of CA cert)
- SQLite queue store for persistent queue state across sessions
- gRPC MessengerService interface (SubmitAuthCode, StreamUpdates) for Tauri integration
- Contact address parsing: simplex.chat/contact link format with dh= and q=c parameters
- AgentInvitation builder: 'I' tag, X448 key pairs for X3DH, connReq URI with correct URL encoding
- E2E encryption layer: NaCl crypto_box with PubHeader format (phVersion=4, '1',',', X25519 SPKI, nonce)
- Background loop: MSG reception confirmed, AgentConfirmation (16KB) received from SimpleX Desktop

#### Infrastructure
- gRPC port 50053 reserved for sgx-simplex sidecar

### Changed - Season 4
- Protocol table in README: SimpleX status updated from "Planned" to "In Development (pre-alpha)"

### Added - Season 3

#### Tor Integration (Arti)
- Native Tor integration using Arti 0.41 (Rust Tor implementation)
- SOCKS5 proxy bridge on 127.0.0.1:19150 for Matrix traffic
- Tor bootstrap with directory consensus caching for fast reconnects
- Exit IP verification via api.ipify.org after bootstrap
- Tor Dashboard in Settings with circuit info and SOCKS proxy status
- Auto-restore: Tor routing mode persists across app restarts
- StatusBanner shows "Connected via Tor (Exit: X.X.X.X)" with verified IP

#### I2P Integration (i2pd Sidecar)
- I2P support using i2pd 2.56.0 (C++) as external sidecar process
- Built-in SOCKS5 proxy on port 4447 (no SAM bridge needed)
- Automatic reseed handled by i2pd internally
- I2P Dashboard in Settings with live data from i2pd webconsole (port 7070)
- Live stats: uptime, bandwidth, routers, floodfills, tunnels, success rate, version
- Dashboard polls every 5 seconds when I2P Settings tab is open
- Invisible process management (CREATE_NO_WINDOW on Windows)
- Watchdog monitors SOCKS5 health with auto-restart on failure
- Background tunnel readiness check via real SOCKS CONNECT to homeserver
- Sync loop starts only after tunnel is confirmed ready (no error spam)
- Cancellation via Arc<AtomicBool> for clean mode switching
- i2pd killed on mode switch, app close, and app crash (no zombie processes)
- VPS server-side: i2pd tunnel config for Tuwunel homeserver on I2P network
- Homeserver reachable at aho2me4wz2...b32.i2p:8448

#### Protocol Routing System
- Three routing modes for Matrix: Direct, Tor, I2P (one-click switching)
- Routing tab in Settings with protocol cards and mode buttons
- Routing config persisted as routing-config.json (auto-migration from tor-routing.json)
- Clean mode transitions: sync cancel -> proxy kill -> client rebuild -> new sync
- RoutingMode enum (Direct/Tor/I2P) replaces old TorMode

#### StatusBanner Component
- Global status banner visible in both Chat and Settings views
- Expandable log panel with chevron toggle (max 50 entries, auto-scroll)
- Timestamped log entries with color coding (green=connected, red=error)
- Minimum 2-second display time per message (no flickering)
- Detail messages from both I2P and Tor bootstrap processes
- Frontend-managed timer (backend sends text only, no time values)
- Separate state management for I2P and Tor (independent events)
- Backwards-compatible: accepts both structured {state, detail} and legacy string events

#### Telegram Improvements
- Telegram sender avatars displayed in chat message bubbles (not just contact list)
- Avatar resolution via TDLib get_user() and profile_photo.small.id
- sender_avatar_url passed through proto, FrontendMessage, and TgNewMessageEvent
- Avatar.svelte handles tg-file: prefix with global cache

#### Emissary Research and Fork
- Tested emissary-core v0.4.0 (Rust I2P implementation) as embedded library
- Discovered three bugs in v0.4.0 (released April 12, 2026):
  - Duration overflow panic in Profile::is_failing() (crashes on second launch)
  - Self-shutdown after ~9 minutes (transport manager channel closes)
  - Transit tunnel panic after ~25 minutes (assert!(false) in tunnel pool)
- Created fork: github.com/saschadaemgen/emissary (branch: fix/duration-overflow)
- Fix: checked_sub instead of panicking subtraction in profile.rs
- All three bugs reported upstream (Issues #339, #340, #341)
- Decision: Migrated to i2pd sidecar for stability

#### License Change
- Changed license from Apache-2.0 to AGPL-3.0-or-later

### Changed - Season 3

- Renamed tor_commands.rs to routing_commands.rs for clarity
- "Tor:" log prefix replaced with "Routing:" for non-Tor-specific operations
- "Sync through Tor timed out" renamed to "Sync timed out (proxy active)"
- Matrix SDK encryption recovery errors suppressed (log level set to off)
- ARCHITECTURE_AND_SECURITY.md expanded with Tor/I2P architecture sections

### Fixed - Season 3

- Sync loop no longer starts before I2P tunnels are ready (eliminates HostUnreachable errors)
- Sync cancelled before i2pd kill on mode switch (no in-flight request errors)
- i2pd process hidden on Windows (no taskbar icon, no notification popup)
- Stale i2pd processes killed on bootstrap, mode switch, and app exit
- sgx-telegram process killed on app close (no zombie sidecar)
- StatusBanner disappears immediately on mode switch to Direct
- No "Restoring Matrix session via I2P" after switching away from I2P
- i2pd webconsole 403 Forbidden fixed (Host header + strictheaders=false)
- Compiler warnings: zero across entire workspace

### Removed - Season 3

- emissary-core and emissary-util dependencies (replaced by i2pd sidecar)
- SAMv3 session code, SOCKS5-to-SAM bridge, manual reseed logic
- All emissary-specific imports and configuration
- Fake/placeholder data in I2P Settings tab
- tor-routing.json (migrated to routing-config.json)

### Added - Season 2

#### Setup Wizard
- First-time setup wizard with 5 animated phases (Welcome, Choose, Matrix, Telegram, Ready)
- Protocol selection: Matrix and Telegram toggleable, minimum one required
- Matrix setup: homeserver selection (simplego.dev, matrix.org, custom), login flow
- Telegram setup: phone/code/2FA with skip option and unregistered number detection
- Ready screen with confetti animation, auto-continue after 4s, manual "Start chatting" button
- Dynamic progress dots adapting to selected protocols
- Wizard re-triggers automatically when all accounts are disconnected
- "Run Setup Wizard" button in About tab for manual re-run
- Matrix registration stub (UI ready, backend UIAA flow planned)

#### Splash Screen and Branding
- Splash screen with SimpleGoX logo on app startup (2s hold, fade out)
- AnimatedBackground component: aurora gradient blobs + floating connected particles
- Shared animated background between splash screen and setup wizard
- Custom SimpleGoX app icon (three-dot triangle on dark circle with blue ring)
- All 14 icon sizes for Windows, Linux, and macOS (including ICO with 16/32/48/256)
- Thick ring design (40px at 512px) for crisp rendering at small taskbar sizes
- Logo color scheme: "Simple" white, "Go" accent color, "X" white

#### Multi-Messenger Architecture
- Protobuf service contract (messenger.proto) defining unified messenger interface
- sgx-proto crate for shared gRPC type generation
- sgx-telegram crate: Telegram sidecar binary with TDLib 1.8.61 via tdlib-rs
- gRPC server-side streaming support for real-time updates (StreamUpdates)
- SidecarManager for spawning and managing external protocol processes
- Automatic sidecar startup on app launch with session detection
- TDLib loadChats pre-loading on AuthorizationState::Ready for reliable chat list after restart
- DownloadAvatar gRPC endpoint for Telegram avatar retrieval via TDLib downloadFile
- Telegram chat type badges (Grp for groups, Ch for channels) in sidebar

#### Telegram Integration
- TDLib authentication flow (phone number, code, 2FA password)
- Telegram login with persistent session (tdlib-data/)
- Telegram chat list loading via gRPC ListChats
- Telegram message loading with pagination loop (getChatHistory)
- Telegram message sending from SimpleGoX to Telegram contacts
- Sender name resolution (own name via get_me, DM partner via chat title)
- Sticker and animated emoji rendering as text emoji
- Telegram logout with session cleanup
- Telegram avatar download pipeline (TDLib downloadFile, Base64 data URL, memory cache)
- Avatar.svelte routes tg-file: URLs to TG downloader, mxc:// to Matrix resolver
- resolveMxcUrl guard rejects tg-file: URIs as safety net
- Reactive resolve spam fix with lastUri tracking

#### Account Management
- Matrix disconnect button in Settings with confirm dialog and encryption key warning
- Full session cleanup on Matrix logout (crypto store, state store, config)
- Retry loop for deleting locked SQLite files after logout
- Settings reset to defaults when all accounts disconnected (accent color, stores, localStorage)
- Default accent color changed to #58a6ff (RGB: 88, 166, 255)

#### Svelte 5 Migration
- Complete frontend migration from vanilla JS to Svelte 5
- Reactive stores for rooms, messages, and UI state
- Component-based architecture (ChatView, RoomList, RoomItem, Avatar, etc.)

#### UI/UX Redesign
- Custom two-part bubble design with info bar and split line
- Circular 56px avatars with quarter-cut effect at bubble edge
- Stacked bubble groups (same sender within 5 min)
- Reply quotes as narrower shields mounted on top of bubbles
- Reactions as shield bar with vertical dividers
- Emoji picker for reactions
- Protocol badges (MX, TG) on every chat in sidebar
- Unified chat list sorting Matrix and Telegram by last activity
- Accent color system with 10 presets and custom hex input

#### Settings Panel
- Fullscreen overlay with blur background and scale/fade animation
- Five tabbed sections: Accounts, Appearance, Privacy, Notifications, About
- Vertical tab navigation with accent color highlights
- Accounts tab: Matrix and Telegram account management with disconnect buttons
- Appearance tab: visual color picker with 2D saturation/lightness field and hue slider
- Privacy tab: read receipts and typing notices toggles
- Notifications tab: desktop notifications and sound toggles
- About tab: version info, protocol badges, links, license, tech stack, run wizard button
- Info tooltips (ui/Tooltip.svelte) on all settings options

#### Matrix Improvements
- Fixed message sender extraction from /messages API response
- Avatar loading via Base64 data URLs (CORS workaround for mxc:// URIs)
- Encrypted message placeholder for undecryptable messages
- Custom event type handling (dev.simplego.iot.status)
- Room/user management with context menus and Element-style dialogs

#### Documentation
- ARCHITECTURE_AND_SECURITY.md: comprehensive security whitepaper covering hardware classes, post-quantum cryptography, crypto-shredding, Tauri vs Electron analysis, competitor comparison, and certification roadmap
- Updated README with hardware roadmap, onboarding features, and security section
- Dev launcher script for Windows (community contribution by Gas Lighter)
- Scripts README with usage documentation

### Removed - Season 2
- Old SettingsOverlay.svelte (replaced by new tabbed Settings)
- Manual Telegram connect/sidecar buttons from sidebar
- 5-second polling timer (replaced by event-driven updates)
- docs/public/ directory (content consolidated into root-level documents)
- Default Tauri app icon (replaced by custom SimpleGoX icon)

### Fixed - Season 2
- Matrix is_own always true bug (sender extraction from raw JSON)
- Telegram message pagination (TDLib returns fewer messages by design)
- Telegram sender display names showing numeric IDs
- Telegram chat list empty on app restart (loadChats must precede getChats)
- Telegram sidecar connection retry with proper delay sequence
- White border on settings overlay (global outline:none + custom focus-visible)
- Accent color not applying to incoming message bubbles
- Avatar position in stacked bubble groups
- Crypto store device ID mismatch after Matrix logout and re-login
- Sync loop continuing after Matrix logout (M_UNKNOWN_TOKEN errors)
- Wizard completing into Settings instead of Chat
- Black screen when all accounts disconnected (splash blocking wizard)
- wizardCompleted guard permanently blocking wizard re-trigger
- Duplicate entries in .gitignore (tdlib-data, dist)

## [0.1.0] - Season 1

### Added
- Initial project structure with Cargo workspace (sgx-core, sgx-terminal, sgx-iot, src-tauri)
- sgx-core crate: Matrix client wrapper (SgxClient), config management, error types
  - Login with password (hidden input via rpassword)
  - Session persistence and restore (TOML config)
  - Cross-signing bootstrap with recovery key generation
  - E2E encrypted message sending and receiving (vodozemac)
  - Sync loop with message callback and typing callback
  - Auto-join on room invitations
  - Send, verify, and logout commands
  - Room summary with unread counts
  - Typing notice sending
  - Read receipt sending
- sgx-terminal crate: CLI client with login, run, send, verify, logout subcommands
- sgx-iot crate: placeholder for IoT companion tools
- Tauri v2 desktop client (src-tauri)
  - Login screen with SimpleGoX branding
  - Chat screen with sidebar (room list) and message area
  - Live message receiving via Tauri events
  - Message sending
  - Typing indicators (send and receive, animated dots)
  - Read receipts (auto-sent on room open)
  - Delivery status checkmarks
  - Unread badges in room list
  - Settings screen (privacy toggles, security info, account info)
  - Sender colors (deterministic per user)
  - Message grouping (same sender within 5 min)
  - Date separators (Today, Yesterday, full date)
  - Dark theme
  - Logout with local data cleanup
- Tuwunel 1.5.1 homeserver deployment at matrix.simplego.dev
- Federation verified: simplego.dev <-> matrix.org
- .well-known delegation for Matrix federation
- Documentation structure (docs/public for git, docs/internal gitignored)
- README, LICENSE (Apache-2.0), CONTRIBUTING guide
- CLAUDE.md and settings.local.json for Claude Code
- Season 1 protocol documents
- Hardware roadmap (internal)
