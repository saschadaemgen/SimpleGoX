#!/usr/bin/env bash
# SimpleGoX - Development Environment Setup for WSL2
# Run this script once after cloning the repository.
#
# Usage:
#   chmod +x scripts/setup-dev.sh
#   ./scripts/setup-dev.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

echo ""
echo "============================================"
echo "  SimpleGoX Development Environment Setup"
echo "============================================"
echo ""

# --- 1. System packages ---
info "Updating system packages..."
sudo apt-get update -qq
sudo apt-get install -y -qq \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    cmake \
    git \
    curl \
    > /dev/null 2>&1
info "System packages installed."

# --- 2. Rust toolchain ---
if command -v rustup &> /dev/null; then
    info "Rust already installed: $(rustc --version)"
    info "Updating Rust toolchain..."
    rustup update stable --no-self-update
else
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    info "Rust installed: $(rustc --version)"
fi

# --- 3. Rust components ---
info "Installing Rust components..."
rustup component add clippy rustfmt

# --- 4. Cargo tools ---
info "Installing cargo-watch (auto-rebuild on file changes)..."
if ! command -v cargo-watch &> /dev/null; then
    cargo install cargo-watch
fi

# --- 5. ARM cross-compilation target (for Raspberry Pi) ---
info "Adding ARM cross-compilation target (aarch64)..."
rustup target add aarch64-unknown-linux-gnu

# --- 6. Verify build ---
info "Running first build..."
echo ""
cd "$(dirname "$0")/.."
cargo build 2>&1

if [ $? -eq 0 ]; then
    echo ""
    info "============================================"
    info "  Setup complete! Everything compiled."
    info "============================================"
    echo ""
    info "Next steps:"
    info "  cargo run -p sgx-terminal -- --help"
    info "  cargo run -p sgx-terminal -- login --homeserver https://matrix.org --user YOUR_USER"
    info "  cargo watch -x 'build'    (auto-rebuild on changes)"
    echo ""
else
    echo ""
    error "Build failed. Check the errors above."
    error "Known issue: matrix-sdk 0.16 + Rust 1.94+ needs #![recursion_limit = \"256\"]"
    error "This should already be set in our source files."
    exit 1
fi
