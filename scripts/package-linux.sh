#!/usr/bin/env bash
# Builds the downloadable Linux package into dist/:
#   WTM-Video-linux-<cpu>.tar.gz   both programs, an install script, the icon and a short note
# Run on Linux:  ./scripts/package-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p wtm_gui -p wtm_cli

CPU="$(uname -m)"                      # x86_64 or aarch64
NAME="WTM-Video-linux-$CPU"
rm -rf "dist/linux" && mkdir -p "dist/linux/$NAME"
cp target/release/wtm-video target/release/wtm-video-gui "dist/linux/$NAME/"
cp scripts/install-linux.sh "dist/linux/$NAME/install.sh"
cp assets/icon-512.png "dist/linux/$NAME/icon-512.png"
cp packaging/READ-ME-FIRST-linux.txt "dist/linux/$NAME/READ ME FIRST.txt"
chmod +x "dist/linux/$NAME/install.sh"

tar -C dist/linux -czf "dist/$NAME.tar.gz" "$NAME"
echo "Built:"
ls -lh "dist/$NAME.tar.gz"
