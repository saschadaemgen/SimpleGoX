# Contributing to SimpleGoX

Thank you for your interest in contributing to SimpleGoX!

## Development Setup

### Prerequisites

- Rust 1.75+ via [rustup](https://rustup.rs)
- Linux or WSL2 on Windows
- Git

### Getting Started

```bash
git clone https://github.com/nicokimmel/SimpleGoX.git
cd SimpleGoX
cargo build
cargo test
```

## Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`

**Scopes:** `core`, `terminal`, `iot`, `widget`, `docs`, `ci`

**Examples:**
```
feat(core): add Matrix client initialization
fix(terminal): handle missing config file gracefully
docs(readme): update build instructions
chore(ci): add GitHub Actions workflow
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Write doc comments (`///`) for all public items
- Add tests for new functionality

## Versioning

**Never change version numbers** in `Cargo.toml` files without explicit permission. Always ask before modifying versions.

## Language

- Code, comments, and documentation: English
- Conversation and Season protocols: German

## License

By contributing, you agree that your contributions will be licensed under Apache-2.0.
