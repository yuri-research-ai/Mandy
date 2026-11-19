---
name: mandy-version-bump
description: Bump Mandy's build suffix (+m1 -> +m2 ...) in every version file when delivering a change. Use before a build, commit or push of Mandy.
---

# Mandy version bump

Mandy's version is `<base>+mN`, for example `0.10.0+m1`. Each delivered change
increases N by one: `+m1` -> `+m2` -> `+m3`. The base (`0.10.0`) does not change
here. Never reuse an N that was already built or pushed.

## Where the version lives

Change the first four by hand and keep them identical; the release script updates `install.sh`:

- `package.json` — `"version"`
- `src-tauri/Cargo.toml` — `version` under `[package]`
- `src-tauri/tauri.conf.json` — `"version"`
- `src-tauri/Cargo.lock` — the `version` line of the `handy` package (the line after `name = "handy"`)
- `install.sh` — `VERSION`, `SHA256_AARCH64` and `SHA256_X64` are written by the release script (see below), not by hand

Editing `Cargo.lock` by hand is fine; running `cargo check` also updates it.

## Steps

1. Read the current N from `src-tauri/tauri.conf.json` and set N+1.
2. Replace the old suffix with the new one in the four files above. Check with:
   `grep -n '0\.10\.0+m' package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json` and the `Cargo.lock` line.
3. The DMG name follows the version: `Mandy_<base>+mN_aarch64.dmg` and `Mandy_<base>+mN_x64.dmg`.
4. Run `cargo check --manifest-path src-tauri/Cargo.toml` to confirm the lock file is consistent.

## Building and staging the release

After the bump, run:

```bash
scripts/release-macos.sh
```

It builds Apple Silicon and Intel, signs the updater bundles with
`~/.tauri/mandy.key` (override with `MANDY_UPDATER_KEY`; never read or print
that file), and stages in `releases/`:

- `Mandy_<base>+mN_{aarch64,x64}.dmg` for `install.sh`;
- `Mandy_<base>+mN_{aarch64,x64}.app.tar.gz` and `.sig` for the in-app updater;
- `latest.json`, the updater feed
  (`https://raw.githubusercontent.com/yuri-research-ai/Mandy/mandy/releases/latest.json`).

It also removes the previous version's files and writes `VERSION`,
`SHA256_AARCH64` and `SHA256_X64` into `install.sh`. Commit all of it together
with the version files, so the feed and `install.sh` at the branch head always
name files that exist at the same commit. The updater compares the `+mN`
suffix numerically (`utils::is_newer_mandy_version`).
