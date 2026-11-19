#!/bin/bash
# Installs or updates Mandy on macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/yuri-research-ai/Mandy/mandy/install.sh | bash
#
# Downloads the DMG for this Mac's architecture from the repository, checks its
# SHA-256, copies Mandy.app into /Applications, strips the quarantine flag and
# clears the stale Accessibility grant that an ad-hoc signed update leaves
# behind. No sudo. Settings, history and models are kept.
#
# Environment overrides:
#   MANDY_INSTALL_DIR  where Mandy.app goes (default /Applications)
#   MANDY_NO_LAUNCH=1  don't open Mandy at the end
set -euo pipefail

# Kept in sync with package.json, Cargo.toml and tauri.conf.json by the
# version bump (see .claude/skills/mandy-version-bump).
VERSION="0.10.0+m5"
SHA256_AARCH64="57197984a0e4754116760f476e2627e3d44f06d340811b387f71b33270a98ca1"
SHA256_X64="3f88c05d49447fe4dc139a07e88fbba956ec44e058fd705d68c1da1d8010d24b"

REPO_RAW="https://raw.githubusercontent.com/yuri-research-ai/Mandy/mandy"
BUNDLE_ID="com.yuri-research-ai.mandy"
INSTALL_DIR="${MANDY_INSTALL_DIR:-/Applications}"
APP="$INSTALL_DIR/Mandy.app"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "Mandy's installer only supports macOS."

case "$(uname -m)" in
  arm64)  ARCH="aarch64"; SHA256="$SHA256_AARCH64" ;;
  x86_64) ARCH="x64";     SHA256="$SHA256_X64" ;;
  *)      die "Unsupported architecture: $(uname -m)" ;;
esac

DMG_NAME="Mandy_${VERSION}_${ARCH}.dmg"
# '+' is kept literal in a URL path by most servers, but not all; encode it.
DMG_URL="$REPO_RAW/releases/${DMG_NAME//+/%2B}"

[ -d "$INSTALL_DIR" ] || die "$INSTALL_DIR does not exist."
[ -w "$INSTALL_DIR" ] || die "$INSTALL_DIR is not writable. Set MANDY_INSTALL_DIR=\$HOME/Applications or fix the folder's permissions."

WORK="$(mktemp -d -t mandy-install)"
MOUNT="$WORK/mnt"
cleanup() {
  if [ -d "$MOUNT" ]; then hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

say "Downloading Mandy $VERSION for $ARCH"
curl -fL --progress-bar -o "$WORK/$DMG_NAME" "$DMG_URL" \
  || die "Download failed: $DMG_URL"

say "Verifying checksum"
ACTUAL="$(shasum -a 256 "$WORK/$DMG_NAME" | cut -d' ' -f1)"
[ "$ACTUAL" = "$SHA256" ] || die "Checksum mismatch for $DMG_NAME. Expected $SHA256, got $ACTUAL. Not installing."

say "Mounting the disk image"
mkdir -p "$MOUNT"
hdiutil attach "$WORK/$DMG_NAME" -nobrowse -readonly -quiet -mountpoint "$MOUNT" \
  || die "Could not mount $DMG_NAME"
[ -d "$MOUNT/Mandy.app" ] || die "Mandy.app not found inside the disk image."

UPDATING=0
if [ -d "$APP" ]; then
  UPDATING=1
  say "Quitting the running Mandy"
  osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || true
  # Give it a moment to exit before the bundle is replaced.
  for _ in 1 2 3 4 5; do
    pgrep -qf "$APP/Contents/MacOS/handy" || break
    sleep 1
  done
  rm -rf "$APP"
fi

say "Installing to $APP"
cp -R "$MOUNT/Mandy.app" "$APP"
hdiutil detach "$MOUNT" -quiet
rmdir "$MOUNT" 2>/dev/null || true

# Files fetched with curl normally carry no quarantine flag, but a DMG that
# was opened in Finder before does. Harmless when there is nothing to remove.
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

if [ "$UPDATING" = 1 ]; then
  # An ad-hoc signed build has a new code identity. The old Accessibility
  # entry still looks enabled but no longer applies, so clear it and let
  # Mandy ask again. Microphone and System Audio Recording are prompted for
  # by macOS on first use.
  say "Clearing the previous Accessibility grant"
  tccutil reset Accessibility "$BUNDLE_ID" >/dev/null 2>&1 || true
fi

say "Installed Mandy $VERSION"
if [ "${MANDY_NO_LAUNCH:-}" != 1 ]; then
  open "$APP"
fi

cat <<EOF

Next:
  1. Grant Accessibility when Mandy asks (System Settings > Privacy & Security).
  2. Approve Microphone on your first dictation, and System Audio Recording
     on your first meeting recording.
EOF
