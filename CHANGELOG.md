# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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

### Removed
- Old SettingsOverlay.svelte (replaced by new tabbed Settings)
- Manual Telegram connect/sidecar buttons from sidebar
- 5-second polling timer (replaced by event-driven updates)
- docs/public/ directory (content consolidated into root-level documents)
- Default Tauri app icon (replaced by custom SimpleGoX icon)

### Fixed
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
