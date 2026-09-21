#!/usr/bin/env bash
# Builds the downloadable macOS packages into dist/:
#   WTM-Video-macOS.zip      the app (Apple Silicon and Intel in one), plus a short how-to-open note
#   wtm-video-cli-macOS.zip  the command-line tool
# Run on a Mac:  ./scripts/package-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS="aarch64-apple-darwin x86_64-apple-darwin"
rustup target add $TARGETS

for target in $TARGETS; do
  cargo build --release --target "$target" -p wtm_gui -p wtm_cli
done

# Glue the two builds into one program that runs natively on either kind of Mac.
mkdir -p target/universal
for program in wtm-video wtm-video-gui; do
  lipo -create -output "target/universal/$program" \
    "target/aarch64-apple-darwin/release/$program" \
    "target/x86_64-apple-darwin/release/$program"
done

BINARY=target/universal/wtm-video-gui ./scripts/bundle-macos.sh

rm -rf dist/macos dist/cli-macos
mkdir -p dist/macos dist/cli-macos
cp -R "target/bundle/WTM Video.app" dist/macos/
cp packaging/READ-ME-FIRST-macos.txt "dist/macos/READ ME FIRST.txt"
cp target/universal/wtm-video dist/cli-macos/wtm-video

# ditto keeps the app's permissions and signature intact, which plain zip can lose.
ditto -c -k --sequesterRsrc dist/macos dist/WTM-Video-macOS.zip
ditto -c -k --sequesterRsrc dist/cli-macos dist/wtm-video-cli-macOS.zip
echo "Built:"
ls -lh dist/*.zip
