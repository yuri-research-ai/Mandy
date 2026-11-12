---
name: mandy-version-bump
description: Bump Mandy's build suffix (+m1 -> +m2 ...) in every version file when delivering a change. Use before a build, commit or push of Mandy.
---

# Mandy version bump

Mandy's version is `<base>+mN`, for example `0.10.0+m1`. Each delivered change
increases N by one: `+m1` -> `+m2` -> `+m3`. The base (`0.10.0`) does not change
here. Never reuse an N that was already built or pushed.

## Where the version lives

Change all five places together, and keep them identical:

- `package.json` — `"version"`
- `src-tauri/Cargo.toml` — `version` under `[package]`
- `src-tauri/tauri.conf.json` — `"version"`
- `src-tauri/Cargo.lock` — the `version` line of the `handy` package (the line after `name = "handy"`)
- `install.sh` — `VERSION`, plus `SHA256_AARCH64` and `SHA256_X64` once the DMGs are built (see below)

Editing `Cargo.lock` by hand is fine; running `cargo check` also updates it.

## Steps

1. Read the current N from `src-tauri/tauri.conf.json` and set N+1.
2. Replace the old suffix with the new one in the four files above. Check with:
   `grep -n '0\.10\.0+m' package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json` and the `Cargo.lock` line.
3. The DMG name follows the version: `Mandy_<base>+mN_aarch64.dmg` and `Mandy_<base>+mN_x64.dmg`.
4. Run `cargo check --manifest-path src-tauri/Cargo.toml` to confirm the lock file is consistent.

## After building the DMGs

Both architectures are built for every version: `bun run tauri build` for
Apple Silicon and `bun run tauri build --target x86_64-apple-darwin` for Intel.
Then:

1. Copy both DMGs into `releases/` and `git rm` the previous version's DMGs.
2. Put their checksums into `install.sh`:
   `shasum -a 256 releases/Mandy_<base>+mN_aarch64.dmg releases/Mandy_<base>+mN_x64.dmg`
   go into `SHA256_AARCH64` and `SHA256_X64`.
3. Commit the DMGs, `install.sh` and the version files together, so the script
   at the branch head always points at a DMG that exists at the same commit.
