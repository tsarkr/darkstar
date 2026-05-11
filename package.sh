#!/bin/bash

# Darkstar Packaging Script v0.1.0

echo "🚀 Preparing Darkstar v0.1.0 packaging..."

# 1. Install cargo-packager if not present
if ! command -v cargo-packager &> /dev/null
then
    echo "📦 Installing cargo-packager..."
    cargo install cargo-packager --locked
fi

# 2. Build release binary
echo "🛠 Building release binary..."
cargo build --release

# 3. Create packages
echo "🎁 Creating installers..."

# Detect OS
OS="$(uname)"
if [ "$OS" == "Darwin" ]; then
    echo "🍏 Creating macOS DMG and App bundle..."
    cargo packager --release --formats dmg,app
elif [ "$OS" == "Linux" ]; then
    echo "💻 Local OS is Linux. Creating AppImage/Deb..."
    cargo packager --release --formats appimage,deb
else
    # This part runs if executed in a Windows-like environment (MinGW/MSYS)
    echo "💻 Creating Windows MSI and NSIS installers..."
    cargo packager --release --formats wix,nsis
fi

echo "✅ Packaging complete! Check target/release/packager/ for output files."
