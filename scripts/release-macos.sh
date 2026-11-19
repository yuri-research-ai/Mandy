#!/bin/bash
# Builds a Mandy release for both Mac architectures and stages it in releases/.
#
#   scripts/release-macos.sh
#
# Run after the version bump (see .claude/skills/mandy-version-bump). It:
#   1. builds Apple Silicon and Intel, signing the updater bundles with
#      the key at $MANDY_UPDATER_KEY (default ~/.tauri/mandy.key, no password);
#   2. copies the DMGs and the signed Mandy.app.tar.gz bundles into releases/
#      and removes the previous version's files from git;
#   3. writes releases/latest.json, the feed the in-app updater reads;
#   4. points install.sh at the new DMGs and their checksums.
# Nothing is committed: review `git status` and commit everything together, so
# the feed and install.sh at the branch head always name files that exist.
set -euo pipefail

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

KEY="${MANDY_UPDATER_KEY:-$HOME/.tauri/mandy.key}"
[ -f "$KEY" ] || die "Updater signing key not found at $KEY."

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' src-tauri/tauri.conf.json)"
PREV="$(sed -n 's/^VERSION="\(.*\)"$/\1/p' install.sh)"
[ -n "$PREV" ] || die "Could not read VERSION from install.sh."
[ "$VERSION" != "$PREV" ] || die "Version $VERSION is already released. Bump it first."

REPO_RAW="https://raw.githubusercontent.com/yuri-research-ai/Mandy/mandy"
NATIVE_BUNDLE="src-tauri/target/release/bundle"
X64_BUNDLE="src-tauri/target/x86_64-apple-darwin/release/bundle"

build() {
  # The key is passed as a path; the bundler reads it itself.
  CMAKE_POLICY_VERSION_MINIMUM=3.5 \
    TAURI_SIGNING_PRIVATE_KEY="$KEY" \
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    bun run tauri build "$@"
}

# Stale bundles from an earlier build must never be picked up as this one.
rm -f "$NATIVE_BUNDLE"/macos/Mandy.app.tar.gz* "$X64_BUNDLE"/macos/Mandy.app.tar.gz*

say "Building Mandy $VERSION for Apple Silicon"
build
say "Building Mandy $VERSION for Intel"
build --target x86_64-apple-darwin

stage() { # <bundle dir> <arch label>
  local bundle="$1" arch="$2"
  local dmg="$bundle/dmg/Mandy_${VERSION}_${arch}.dmg"
  local tarball="$bundle/macos/Mandy.app.tar.gz"
  [ -f "$dmg" ] || die "Missing $dmg"
  [ -f "$tarball" ] && [ -f "$tarball.sig" ] || die "Missing signed updater bundle in $bundle/macos"
  cp "$dmg" "releases/Mandy_${VERSION}_${arch}.dmg"
  cp "$tarball" "releases/Mandy_${VERSION}_${arch}.app.tar.gz"
  cp "$tarball.sig" "releases/Mandy_${VERSION}_${arch}.app.tar.gz.sig"
}

say "Copying artifacts into releases/"
stage "$NATIVE_BUNDLE" aarch64
stage "$X64_BUNDLE" x64

say "Removing $PREV from releases/"
for old in releases/Mandy_"${PREV}"_*; do
  [ -e "$old" ] || continue
  git rm -q --cached --ignore-unmatch "$old"
  rm -f "$old"
done

say "Writing releases/latest.json"
python3 - "$VERSION" "$REPO_RAW" <<'EOF'
import json, sys
from datetime import datetime, timezone

version, repo_raw = sys.argv[1], sys.argv[2]

def platform(arch):
    name = f"Mandy_{version}_{arch}.app.tar.gz"
    with open(f"releases/{name}.sig") as f:
        signature = f.read().strip()
    # '+' is kept literal in a URL path by most servers, but not all; encode it.
    return {"url": f"{repo_raw}/releases/{name.replace('+', '%2B')}", "signature": signature}

feed = {
    "version": version,
    "notes": f"Mandy {version}",
    "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {
        "darwin-aarch64": platform("aarch64"),
        "darwin-x86_64": platform("x64"),
    },
}
with open("releases/latest.json", "w") as f:
    json.dump(feed, f, indent=2)
    f.write("\n")
EOF

say "Pointing install.sh at $VERSION"
SHA_ARM="$(shasum -a 256 "releases/Mandy_${VERSION}_aarch64.dmg" | cut -d' ' -f1)"
SHA_X64="$(shasum -a 256 "releases/Mandy_${VERSION}_x64.dmg" | cut -d' ' -f1)"
sed -i '' \
  -e "s/^VERSION=\".*\"$/VERSION=\"$VERSION\"/" \
  -e "s/^SHA256_AARCH64=\".*\"$/SHA256_AARCH64=\"$SHA_ARM\"/" \
  -e "s/^SHA256_X64=\".*\"$/SHA256_X64=\"$SHA_X64\"/" \
  install.sh

git add releases/ install.sh
say "Staged Mandy $VERSION. Review with 'git status' and commit together with the version bump."
