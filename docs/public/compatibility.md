# SimpleGoX Compatibility

## Supported Protocols

| Protocol | Status | Version | Implementation |
|:---------|:-------|:--------|:---------------|
| Matrix | Working | CS API v1.11+ | Native Rust via matrix-rust-sdk 0.16 |
| Telegram | Working | TDLib 1.8.61 | Isolated sidecar via tdlib-rs 1.3.0 |
| SimpleX | Planned (Season 3) | - | Native Rust SMP (planned) |
| WhatsApp | Planned (Season 4) | - | EU DMA interoperability (planned) |

## Matrix Protocol

| Feature | Status |
|:--------|:-------|
| Client-Server API v1.11+ | Supported |
| E2E Encryption (Olm/Megolm) | Supported |
| Cross-Signing | Supported |
| Sliding Sync (MSC4186) | Supported |
| Room Version 12 | Supported |
| Key Backup with Recovery Key | Supported |
| Typing Indicators | Supported |
| Read Receipts | Supported |
| Reactions | Supported |
| Reply/Edit/Redact | Supported |
| Token-based Registration | Planned |

## Telegram Features

| Feature | Status |
|:--------|:-------|
| Authentication (phone + code + 2FA) | Supported |
| Session persistence (auto-reconnect) | Supported |
| Private chats | Supported |
| Group chats | Supported |
| Supergroups | Supported |
| Channels | Supported |
| Text messages (send + receive) | Supported |
| Sticker/animated emoji (as text) | Supported |
| Photo messages | Display planned |
| Video messages | Display planned |
| Voice notes | Display planned |
| Real-time updates (StreamUpdates) | Supported |
| Account signup (new account) | Supported |
| Multi-account | Planned |

## Matrix Homeserver Compatibility

| Homeserver | Compatible | Notes |
|:-----------|:-----------|:------|
| matrix.org (public) | Yes | Standard CS API |
| Synapse (self-hosted) | Yes | Most widely deployed |
| Tuwunel (self-hosted) | Yes | Lightweight Rust server |
| Element Server Suite | Yes | Enterprise deployment |
| Dendrite | Yes | Note: Dendrite is deprecated |
| Conduit/Conduwuit | Yes | Lightweight alternative |

## Institutional Matrix Deployments

SimpleGoX uses the standard Matrix Client-Server API. Compatibility with institutional deployments depends on the operator's access policies.

| Deployment | Protocol Compatible | Access |
|:-----------|:-------------------|:-------|
| BundesMessenger (German government) | Yes | Requires operator approval |
| BwMessenger (German military) | Yes | Requires Bundeswehr approval |
| Tchap (French government) | Yes | Requires approval |
| TI-Messenger (German healthcare) | Yes | Requires gematik certification |
| NATO NI2CE | Yes | Requires NATO approval |
| Corporate deployments | Yes | Requires admin approval |

## Matrix Client Interoperability

SimpleGoX can communicate with any Matrix client:

| Client | Interoperable | E2E Encryption |
|:-------|:-------------|:---------------|
| Element Web/Desktop | Yes | Yes |
| Element X (Android/iOS) | Yes | Yes |
| FluffyChat | Yes | Yes |
| Fractal | Yes | Yes |
| NeoChat | Yes | Yes |
| SchildiChat | Yes | Yes |
| Nheko | Yes | Yes |

## Telegram Client Interoperability

SimpleGoX connects as a standard Telegram client via TDLib. Messages sent from SimpleGoX appear as regular messages in:

| Client | Interoperable |
|:-------|:-------------|
| Telegram (Android/iOS) | Yes |
| Telegram Desktop | Yes |
| Telegram Web | Yes |

## System Requirements

### Desktop Client
- Windows 10/11 (x86_64) or Linux (x86_64/aarch64)
- Microsoft Edge WebView2 Runtime (Windows, usually pre-installed)
- 4 GB RAM recommended
- Rust 1.85+ for building from source
- Node.js 20+ for building from source

### Telegram Sidecar
- TDLib DLLs (auto-downloaded via download-tdlib feature):
  tdjson.dll, libcrypto-3-x64.dll, libssl-3-x64.dll, zlib1.dll
