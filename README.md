# Rustpackages

A Rust-based package manager (**mpkg**) designed to be portable across Rust OS projects (e.g. Ferrite, CraftOS).

## Design principles

- **`root: &Path` everywhere** — all core operations take a target root path, so the core runs against any rootfs (or a fake rootfs in unit tests on the host — no VM required).
- **`.mpk` package format** — tar+zstd archive containing a `pkg.toml` manifest; gui packages declare a `desktop = "app.desktop"` entry so desktop environments can pick them up automatically.
- **Signed repositories** — ed25519-signed repo index.
- **Transactional installs** — resolve → download → verify → stage → atomic swap, with append-only history and rollback. Critical packages (kernel, glibc) use an offline-finish pattern staged for next boot.

## Workspace layout (planned)

- `crates/mpkg-core` — rootfs-agnostic core library (resolve, verify, stage, swap, history)
- `crates/mpkg-cli` — command-line interface: `install / remove / upgrade / list / verify / search / update / history / rollback / repo`

## License

MIT
