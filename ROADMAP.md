# Roadmap — making mpkg usable

Goal: a working, installable Rust package manager (`mpkg`) that Ferrite and
CraftOS can embed as a library. Ordered by dependency — each phase unblocks
the next.

## Phase 1 — Core foundations (library-usable, no network)
- [ ] `mpkg-core`: manifest parsing (`pkg.toml` → typed struct, serde + toml)
- [ ] `.mpk` archive read/write (tar + zstd, deterministic layout)
- [ ] Install/remove on a target root (`root: &Path`), staged + atomic swap
- [ ] File ownership database (which package owns which path) + conflict detection
- [ ] Append-only history log + `rollback`
- [ ] 100% unit tests against a fake rootfs on the host (no VM)

## Phase 2 — Repositories & signing
- [ ] Repo index format (signed, versioned) with ed25519 signatures
- [ ] `mpkg update` — fetch + verify index
- [ ] `mpkg search` / `mpkg info` against the index
- [ ] Key management (trust store, key rotation)

## Phase 3 — Dependency resolution
- [ ] Version constraints in manifests (semver ranges)
- [ ] Dependency graph resolution + topological install order
- [ ] `mpkg upgrade` (full and per-package)
- [ ] Lockfile for reproducible installs

## Phase 4 — CLI polish
- [ ] Full command surface: `install remove upgrade list verify search update history rollback repo`
- [ ] Human-readable output + `--json` mode for GUI embedding
- [ ] Progress reporting (download/install callbacks) for a Ferrite settings UI

## Phase 5 — Integration
- [ ] Critical-package offline-finish pattern (kernel, libc staged for next boot)
- [ ] `.desktop` file registration so installed GUI apps appear in the Ferrite dock
- [ ] Consume as a workspace crate from Ferrite (`ferrite-pkg` wrapper)
- [ ] Consume from CraftOS as the system package layer

## Phase 6 — Hardening
- [ ] Sandboxing / privilege separation for install operations
- [ ] Delta updates and binary diffs to shrink downloads
- [ ] Mirror support and offline `.mpk` installs from local media
