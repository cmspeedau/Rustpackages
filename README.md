# Rustpackages

A Rust-native package manager (`mpkg`) for the Ferrite/CraftOS ecosystem — built as a
standalone workspace, designed to be rootfs-agnostic so it can manage any target root
(not just `/`).

**License: MIT** — see [LICENSE](LICENSE).

## Workspace layout

| Crate | Purpose |
|---|---|
| `crates/mpkg-core` | Rootfs-agnostic library: manifests, `.mpk` archives, transactional store, repo index, dependency resolver, remote fetching, signing |
| `crates/mpkg-cli` | The `mpkg` command-line tool |

## Package format

A package is a `.mpk` file: a zstd-compressed tar containing a `pkg.toml` manifest
at the root plus the payload files (e.g. `usr/bin/foo`, `usr/share/...`).

```toml
[package]
name = "hello"
version = "1.0.0"
description = "example"
arch = "x86_64"

[dependencies]
# name + semver requirement are separate fields:
# dependencies = [{ name = "libfoo", version = "^1.0" }]
```

## On-disk layout (under `--root`, default `/`)

```
var/lib/mpkg/
├── db/<name>.json      # installed record: manifest + per-file sha256
├── history.jsonl       # append-only operation log
├── mpkg.lock           # exact resolved set (reproducible installs)
├── cache/              # downloaded .mpk files
├── repos/              # cached repo indexes + repos.toml
├── keys/               # ed25519 signing keypairs
├── staging/            # transient extraction area
└── trash/<name>/       # removed file contents (enables rollback)
```

## Commands

```
mpkg install <path.mpk | repo-url/name>   install from file or resolve via repo index
mpkg remove <name>                        remove (files kept in trash for rollback)
mpkg upgrade <repo-url>                   upgrade all installed pkgs to latest in repo
mpkg list [--json]                        list installed packages
mpkg verify [name]                        check files against recorded sha256
mpkg search <repo-url> <query>            search a repo index
mpkg update <repo-url>                    cache a remote index locally
mpkg history                              show operation log
mpkg rollback                             undo the last operation (restores removed files)
mpkg repo add <name> <url> / repo list    manage configured repos
mpkg keygen <name>                        generate an ed25519 keypair for signing indexes
```

Global flags: `--root <path>` (target rootfs, default `/`), `--json` (machine-readable output).

## Design notes

- **Transactional store**: installs stage + move files, then write the db record and
  history; removes move files to trash instead of deleting so `rollback` fully restores them.
- **Reproducible**: `mpkg.lock` pins name/version/sha256 of every installed package.
- **Verifiable**: every installed file's sha256 is recorded; `mpkg verify` detects tampering.
- **Repos**: `index.toml` lists packages with sha256 + URL; indexes can be ed25519-signed.
- **Remote fetching**: `http(s)://` and `file://` URLs (HTTP behind the default-on
  `http` cargo feature, so core builds without networking if needed).

## Status & roadmap

See [ROADMAP.md](ROADMAP.md) for what remains (delta updates, mirrors, sandboxing,
Ferrite integration). Not yet integrated into the OS — this repo is the standalone
foundation.

## Development

```
cargo build
cargo test
```
