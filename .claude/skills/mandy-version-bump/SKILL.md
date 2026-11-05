---
name: mandy-version-bump
description: Bump Mandy's build suffix (+m1 -> +m2 ...) in every version file when delivering a change. Use before a build, commit or push of Mandy.
---

# Mandy version bump

Mandy's version is `<base>+mN`, for example `0.10.0+m1`. Each delivered change
increases N by one: `+m1` -> `+m2` -> `+m3`. The base (`0.10.0`) does not change
here. Never reuse an N that was already built or pushed.

## Where the version lives

Change all four places together, and keep them identical:

- `package.json` — `"version"`
- `src-tauri/Cargo.toml` — `version` under `[package]`
- `src-tauri/tauri.conf.json` — `"version"`
- `src-tauri/Cargo.lock` — the `version` line of the `handy` package (the line after `name = "handy"`)

Editing `Cargo.lock` by hand is fine; running `cargo check` also updates it.

## Steps

1. Read the current N from `src-tauri/tauri.conf.json` and set N+1.
2. Replace the old suffix with the new one in the four files above. Check with:
   `grep -n '0\.10\.0+m' package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json` and the `Cargo.lock` line.
3. The DMG name follows the version: `Mandy_<base>+mN_aarch64.dmg`.
4. Run `cargo check --manifest-path src-tauri/Cargo.toml` to confirm the lock file is consistent.
