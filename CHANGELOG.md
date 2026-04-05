# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Initial project structure with Cargo workspace (sgx-core, sgx-terminal, sgx-iot, src-tauri)
- `sgx-core` crate: Matrix client wrapper (SgxClient), config management, error types
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
- `sgx-terminal` crate: CLI client with login, run, send, verify, logout subcommands
- `sgx-iot` crate: placeholder for IoT companion tools
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
  - Dark theme (#0f0f1a background, #45bdd1 primary)
  - Logout with local data cleanup
- Tuwunel 1.5.1 homeserver deployment at matrix.simplego.dev
- Federation verified: simplego.dev <-> matrix.org
- .well-known delegation for Matrix federation
- Documentation structure (docs/public for git, docs/internal gitignored)
- README, LICENSE (Apache-2.0), CONTRIBUTING guide
- CLAUDE.md and settings.local.json for Claude Code
- Season 1 protocol documents
- Hardware roadmap (internal)
