# SimpleGoX Compatibility

## Matrix Protocol

SimpleGoX implements the Matrix Client-Server API and is compatible with any homeserver that supports it.

| Feature | Status |
|---|---|
| Client-Server API v1.11+ | Supported |
| E2E Encryption (Olm/Megolm) | Supported |
| Cross-Signing | Supported |
| Sliding Sync (MSC4186) | Supported |
| Room Version 12 | Supported |
| Token-based Registration | Planned |

## Homeserver Compatibility

| Homeserver | Compatible | Notes |
|---|---|---|
| matrix.org (public) | Yes | Standard CS API |
| Synapse (self-hosted) | Yes | Most widely deployed |
| Tuwunel (self-hosted) | Yes | Lightweight Rust server |
| Element Server Suite | Yes | Enterprise deployment |
| Dendrite | Yes | Note: Dendrite is deprecated |

## Institutional Matrix Deployments

SimpleGoX uses the standard Matrix Client-Server API. Compatibility with institutional deployments depends on the operator's access policies.

| Deployment | Protocol Compatible | Access |
|---|---|---|
| BundesMessenger (German government) | Yes | Requires operator approval |
| BwMessenger (German military) | Yes | Requires Bundeswehr approval |
| Tchap (French government) | Yes | Requires approval |
| TI-Messenger (German healthcare) | Yes | Requires gematik certification |
| NATO NI2CE | Yes | Requires NATO approval |
| Corporate deployments | Yes | Requires admin approval |

## Client Interoperability

SimpleGoX can communicate with any Matrix client:

| Client | Interoperable | E2E Encryption |
|---|---|---|
| Element Web/Desktop | Yes | Yes |
| Element X (Android/iOS) | Yes | Yes |
| FluffyChat | Yes | Yes |
| Fractal | Yes | Yes |
| NeoChat | Yes | Yes |
| SchildiChat | Yes | Yes |
| Nheko | Yes | Yes |

## Hardware Requirements

### Phase 1 (Current)
- Any Linux system with Rust 1.75+
- Raspberry Pi 4/5 (4 GB RAM recommended)
- x86_64 or aarch64 architecture

### Phase 2 (Planned)
- Custom hardware with hardened Linux
- Touchscreen support
- Physical security features
