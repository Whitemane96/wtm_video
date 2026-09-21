#!/usr/bin/env bash
# Installs WTM Video for the current user: both programs, the icon, and an
# entry in your application menu.
#
#   ./install-linux.sh              install (from a source checkout this builds first)
#   ./install-linux.sh --uninstall  remove it again
#
# Installs under ~/.local by default; set PREFIX to change that.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor/512x512/apps"

refresh_caches() {
  update-desktop-database "$APPS" 2>/dev/null || true
  gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN/wtm-video" "$BIN/wtm-video-gui" "$APPS/wtm-video.desktop" "$ICONS/wtm-video.png"
  refresh_caches
  echo "Removed WTM Video from $PREFIX"
  exit 0
fi

if [ -x "$HERE/wtm-video-gui" ]; then
  # Running from a downloaded package: the programs sit next to this script.
  SRC="$HERE"
  ICON="$HERE/icon-512.png"
else
  # Running from a source checkout: build first.
  cd "$HERE/.."
  cargo build --release
  SRC="target/release"
  ICON="assets/icon-512.png"
fi

mkdir -p "$BIN" "$APPS" "$ICONS"
install -m 755 "$SRC/wtm-video" "$BIN/wtm-video"
install -m 755 "$SRC/wtm-video-gui" "$BIN/wtm-video-gui"
install -m 644 "$ICON" "$ICONS/wtm-video.png"

cat > "$APPS/wtm-video.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=WTM Video
Comment=Download videos from YouTube and other sites
Exec="$BIN/wtm-video-gui"
Icon=wtm-video
Terminal=false
Categories=AudioVideo;Network;
StartupWMClass=wtm-video
DESKTOP

refresh_caches
echo "Installed WTM Video to $PREFIX"
echo "Find it in your application menu, or run: wtm-video-gui"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "Note: $BIN is not on your PATH, so add it to run 'wtm-video' from a terminal." ;;
esac
