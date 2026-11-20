# Packaging applications for Rustpackages (`mpkg`)

A package is a single `.mpk` file. Build one from a directory with the
reference layout below — no special tooling beyond `tar` + `zstd` is
required (the `mpkg-core` library exposes `archive::build_package` for
programmatic builds).

## Package layout

```
myapp/
├── pkg.toml              # manifest (required, at the archive root)
└── usr/                  # payload, extracted relative to the target root
    ├── bin/myapp
    └── share/applications/myapp.desktop   # optional, for GUI apps
```

## Manifest (`pkg.toml`)

```toml
[package]
name = "myapp"            # lowercase letters, digits, '-', '_' only
version = "1.2.0"         # semver
arch = "x86_64"           # defaults to build arch
description = "What it does"
license = "MIT"
homepage = "https://example.com/myapp"

# Optional dependencies (installed/resolved first):
# [[dependencies]]
# name = "libfoo"
# version = "^1.0"

# Optional: desktop entry for GUI launchers
# desktop = "usr/share/applications/myapp.desktop"

# Optional: mark critical (offline-finish / stage-for-next-boot semantics)
# critical = true
```

## Building the `.mpk`

An `.mpk` is a **zstd-compressed tar** of the directory above:

```sh
tar -C myapp -cf - . | zstd -19 -o myapp-1.2.0.mpk
```

Naming convention: `<name>-<version>.mpk` (the mirror publisher relies on it).

## Rules & validation

- `name` must be non-empty and match `[a-z0-9_-]+`.
- `version` must be valid semver; dependency requirements must be valid
  semver reqs (e.g. `^1.0`, `>=2.0, <3.0`, `*`).
- Payload paths are extracted relative to the target root (`--root`),
  so use `usr/...`, `etc/...`, etc. — never absolute paths in the tar.
- Every installed file's sha256 is recorded at install time;
  `mpkg verify` will flag any later modification.

## Publishing to a mirror

1. Drop the `.mpk` into your packages dir.
2. Run `docker/publish.sh <packages-dir> <output-dir>` to generate
   `index.toml` + `pkgs/`.
3. Serve `<output-dir>` with the docker mirror (`docker compose up -d`),
   or any static file server.
4. Clients then use:
   ```sh
   mpkg update http://mirror:8080
   mpkg search http://mirror:8080 myapp
   mpkg install http://mirror:8080/pkgs/myapp-1.2.0.mpk
   ```

## Signing (optional but recommended)

```sh
mpkg keygen myrepo          # writes an ed25519 keypair under var/lib/mpkg/keys/
```

Sign `index.toml` with the secret key and publish the signature alongside;
clients can then verify index integrity before trusting it.
