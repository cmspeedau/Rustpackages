# Roadmap — making mpkg usable

Goal: a working, installable Rust package manager (`mpkg`) that Ferrite and
CraftOS can embed as a library. Ordered by dependency — each phase unblocks
the next. Items marked *(parity)* come from the apk/deb/pacman comparison —
features expected of a real-world package manager.

## Phase 1 — Core foundations (library-usable, no network) ✅
- [x] `mpkg-core`: manifest parsing (`pkg.toml` → typed struct, serde + toml)
- [x] `.mpk` archive read/write (tar + zstd, deterministic layout)
- [x] Install/remove on a target root (`root: &Path`), staged + atomic swap
- [x] File ownership database (which package owns which path) + conflict detection *(parity: deb/pacman error, apk overwrite)*
- [x] Append-only history log + `rollback`
- [x] 100% unit tests against a fake rootfs on the host (no VM)

## Phase 2 — Repositories & signing *(IN PROGRESS)*
- [x] Repo index format (signed, versioned) with ed25519 signatures
- [x] `mpkg update` — fetch + verify index
- [x] `mpkg search` / `mpkg info` against the index *(parity: `apk info`, `apt show`, `pacman -Si`, incl. file listing of uninstalled packages)*
- [ ] Key management (trust store, key rotation)
- [ ] Download resume (partial `.mpk` continuation) + parallel downloads *(parity)*

## Phase 3 — Dependency resolution
- [ ] Version constraints in manifests (semver ranges)
- [ ] Full dependency graph resolution (conflicts detected before install, satisfying set chosen — not greedy) *(parity)*
- [ ] Virtual packages / `provides` (e.g. `mail-transport-agent`, `libfoo.so.1`) *(parity)
- [ ] `mpkg upgrade` (full and per-package)
- [ ] Lockfile for reproducible installs
- [ ] Orphan detection + autoremove of unneeded dependencies *(parity: `pacman -Qtd`, `apt autoremove`)*

## Phase 4 — CLI polish
- [ ] Full command surface: `install remove upgrade list verify search update history rollback repo`
- [ ] Human-readable output + `--json` mode for GUI embedding
- [ ] Progress reporting (download/install callbacks) for a Ferrite settings UI

## Phase 5 — Integration
- [ ] Maintainer scripts: pre/post-install hooks (daemon restart, user creation) — initially allowlist-only, never arbitrary root shell *(parity: deb pre/postinst, pacman .install, apk hooks)*
- [ ] Triggers: run actions when paths change (icon cache, desktop database, ldconfig) *(parity: apk triggers, pacman hooks)*
- [ ] Critical-package offline-finish pattern (kernel, libc staged for next boot)
- [ ] `.desktop` file registration so installed GUI apps appear in the Ferrite dock
- [ ] Consume as a workspace crate from Ferrite (`ferrite-pkg` wrapper)
- [ ] Consume from CraftOS as the system package layer

## Phase 6 — Hardening
- [ ] Sandboxing / privilege separation for install operations and maintainer scripts
- [ ] Delta updates and binary diffs to shrink downloads
- [ ] Mirror support and offline `.mpk` installs from local media (docker/nginx setup in `docker/`)
- [ ] Multi-arch support (foreign architectures) *(parity: deb multiarch)*
