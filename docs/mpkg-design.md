# mpkg — High-level Design

## Purpose
A Rust-native, desktop-first package manager designed for CraftOS/Ferrite. The goal is **not** to replace apk/pacman/dpkg, but to offer a clean, Rust-idiomatic alternative for a Rust-based OS where safety, concurrency, and embeddability matter more than extreme performance.

## Core Design Goals

| Goal | Why | How |
|------|-----|-----|
| **Memory Safety** | OS-level package manager parses untrusted repo indexes, manifest, signatures, archive headers, maintainer scripts. C-based managers have a long history of CVEs (buffer overflows, integer overflows, path traversals). Rust eliminates entire classes of these bugs at compile time. | Entire mpkg codebase written in Rust; no unsafe except in thin FFI wrappers for kernel APIs (e.g., seccomp, namespaces). |
| **Fearless Concurrency** | Parallel downloads, background index refreshes, real-time progress callbacks to Ferrite UI. | Rust’s ownership model lets us spin up lightweight tasks for downloads/verification without fear of data races. |
| **Embeddable Library** | mpkg should be usable as a Rust crate inside Ferrite/CraftOS, not just a CLI. | mpkg exposes a public API (`mpkg::install`, `mpkg::query`, `mpkg::repo`) that can be called from GUI code without FFI or unsafe. |
| **Desktop-first UX** | Focus on desktop workflows: app stores, app icons, desktop entries, mime types, font caches, icon caches, ldconfig triggers. | Built-in support for maintainer scripts (`preinst`, `postinst`, `prerm`, `postrm`) with a sandboxed runner (seccomp + namespaces). Triggers for desktop-db, icon-cache, font-cache, gtk-update-icon-cache, ldconfig, etc. |
| **Deterministic, Signed Repos** | Prevent supply-chain attacks and repo spoofing. | Repo metadata signed with Ed25519; clients verify signatures before parsing. Index format is deterministic and hash-based to prevent tampering. |
| **Lockfile & Atomic Installs** | Reproducible installs, rollbacks, and system recovery. | Lockfile specifies exact versions of packages and their dependencies. Installs are atomic: either all files land in place, or the transaction is rolled back. |
| **Parallel Downloads & Resume** | Faster installs on slow or flaky connections. | Downloads are chunked; failed chunks resume from last byte. Parallel downloads for multiple packages at once. |
| **Provides & Virtual Packages** | Allow packages to provide capabilities that others can depend on (e.g., `libc.so.6`, `pkgconfig`, `python3`). | Supports `provides` in manifest; dependency resolver treats them as equality constraints. |
| **Dependency Resolution** | Correctly resolve dependencies, conflicts, and orphans. | Resolution is greedy at first, then backtracks if conflicts arise. Supports `conflict` and `replaces` fields. |
| **Multi-arch Support** | Support for multiple CPU architectures (x86_64, aarch64, riscv64, etc.). | Package archives include architecture-specific paths; resolver picks the correct one. |
| **Sandboxed Scripts** | Run maintainer scripts safely in a restricted environment. | Scripts run in a containerized environment with seccomp, namespaces, and read-only filesystem overlay. Only allowlisted syscalls are permitted. |

## High-level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        mpkg (Rust crate)                     │
├─────────────────┬─────────────────┬─────────────────┬───────┤
│   mpkg CLI      │   mpkg API       │   Ferrite GUI   │       │
│ (user-facing)   │ (library)        │ (calls mpkg API)│       │
└────────┬────────┴────────┬────────┴────────┬────────┘       │
         │                 │                 │
         ▼                 ▼                 ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   Repo Index    │ │ Dependency      │ │   Maintainer    │
│   (signed,      │ │ Resolver        │ │   Scripts       │
│    deterministic)│ │                 │ │   (sandboxed)   │
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                 │                 │
         ▼                 ▼                 ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   Package       │ │   Archive       │ │   Trigger       │
│   Archive       │ │   Verifier      │ │   Manager       │
│   (tar.zst +    │ │ (checksums,     │ │ (desktop-db,    │
│    signatures)  │ │  signatures)    │ │  icon-cache,    │
└────────┬────────┘ └────────┬────────┘ │  ldconfig, etc.)│
         │                 │           └────────┬────────┘
         ▼                 ▼                    │
┌─────────────────┐ ┌─────────────────┐          │
│   Download      │ │   Sandbox       │          │
│   Manager       │ │   (seccomp +     │◄─────────┘
│   (parallel,    │ │   namespaces)   │
│    resume)      │ └─────────────────┘
└─────────────────┘
```

### Key Components

| Component | Description |
|-----------|-------------|
| **Repo Index** | Deterministic, signed JSON/YAML/TOML index of all packages in a repo. Contains package name, version, architecture, dependencies, provides, conflicts, maintainer scripts, archive checksums, and signatures. |
| **Dependency Resolver** | Greedy resolver with backtracking. Uses `Cargo`-style semver for dependencies, but allows `conflict` and `replaces` for advanced cases. Outputs a lockfile. |
| **Package Archive** | Each package is a single file: a tar.zst archive containing:
- `bin/`, `lib/`, `share/` directories (standard Unix layout)
- `meta/` directory (manifest, signatures, triggers)
- Optional maintainer scripts (`preinst`, `postinst`, etc.)
- Optional `provides` and `replaces` files for virtual packages |
| **Download Manager** | Parallel downloads with resume support. Chunked transfers and integrity checks. |
| **Archive Verifier** | Verifies archive checksums and signatures before extraction. |
| **Sandbox** | Runs maintainer scripts in a restricted environment with seccomp and namespaces. Only allowlisted syscalls are permitted (e.g., no `execve`, no `mount`). |
| **Trigger Manager** | Runs post-install triggers for desktop-db, icon-cache, font-cache, ldconfig, etc. Only runs triggers that are explicitly listed in the package manifest. |

## Repo Format

### Signed Repo Index
```json
{
  "repo": "craftos",
  "arch": ["x86_64", "aarch64"],
  "timestamp": "2026-11-20T19:20:00Z",
  "packages": [
    {
      "name": "craftos-desktop",
      "version": "0.1.0",
      "arch": "x86_64",
      "archive": "craftos-desktop-0.1.0-x86_64.tar.zst",
      "checksum": "sha256:...",
      "size": 42000000,
      "dependencies": ["libc>=0.1.0", "gtk3>=3.24.0"],
      "provides": ["desktop-environment"],
      "conflicts": [],
      "replaces": [],
      "maintainer_scripts": ["preinst", "postinst"],
      "triggers": ["desktop-db", "icon-cache"],
      "signature": "ed25519:..."
    }
  ],
  "signature": "ed25519:..."
}
```

### Package Manifest (inside archive)
```toml
[package]
name = "craftos-desktop"
version = "0.1.0"
arch = "x86_64"

[dependencies]
libc = ">=0.1.0"
gtk3 = ">=3.24.0"

[provides]
"desktop-environment" = ""

[conflicts]
"gnome-shell" = ""

[replaces]
""

[maintainer_scripts]
preinst = ""
postinst = ""

[triggers]
"desktop-db" = ""
"icon-cache" = ""
```

## Lockfile Format
```toml
[lock]
repo = "craftos"
arch = "x86_64"
installed = [
  { name = "libc", version = "0.1.0", archive = "libc-0.1.0-x86_64.tar.zst" },
  { name = "gtk3", version = "3.24.33", archive = "gtk3-3.24.33-x86_64.tar.zst" },
  { name = "craftos-desktop", version = "0.1.0", archive = "craftos-desktop-0.1.0-x86_64.tar.zst" }
]
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `mpkg install <pkg>` | Install a package and its dependencies. |
| `mpkg remove <pkg>` | Remove a package and its dependencies if no one else depends on them. |
| `mpkg query <pkg>` | Query installed packages and repo packages. |
| `mpkg update` | Refresh repo index from remote. |
| `mpkg upgrade` | Upgrade installed packages to latest versions. |
| `mpkg download <pkg>` | Download package archive without installing. |
| `mpkg verify <pkg>` | Verify package archive integrity and signatures. |

## Security Model

- **No root required for user installs.** Packages install to `~/.local/mpkg` by default, with optional `--root` flag for system-wide installs.
- **Sandboxed maintainer scripts.** Even if a script runs `rm -rf /`, it only affects the sandboxed filesystem overlay.
- **Signed repos.** Clients verify repo index and package signatures before parsing or extracting anything.
- **Deterministic index.** Prevents repo spoofing by ensuring the index is hash-based and deterministic.

## Comparison to apk/pacman/dpkg

| Feature | mpkg | apk (Alpine) | pacman (Arch) | dpkg (Debian) |
|---------|------|-------------|---------------|---------------|
| **Language** | Rust | C | C | C |
| **Memory Safety** | ✅ (compile-time) | ❌ (runtime) | ❌ (runtime) | ❌ (runtime) |
| **Concurrency** | ✅ (fearless) | ❌ (mutexes) | ❌ (mutexes) | ❌ (mutexes) |
| **Signed Repos** | ✅ (Ed25519) | ✅ (RSA) | ❌ (optional) | ✅ (optional) |
| **Lockfile** | ✅ | ❌ | ❌ | ❌ |
| **Atomic Installs** | ✅ | ✅ | ✅ | ✅ |
| **Maintainer Scripts** | ✅ (sandboxed) | ✅ | ✅ | ✅ |
| **Virtual Packages** | ✅ (`provides`) | ✅ (`provides`) | ✅ (`provides`) | ✅ (`Provides`) |
| **Multi-arch** | ✅ | ✅ | ✅ | ✅ |
| **Desktop Triggers** | ✅ (built-in) | ❌ | ❌ | ❌ |
| **Embeddable** | ✅ (Rust crate) | ❌ (C lib) | ❌ (C lib) | ❌ (C lib) |

## Next Steps

- Implement repo index signing and verification.
- Build dependency resolver with backtracking.
- Write sandbox runner with seccomp + namespaces.
- Implement maintainer script runner and trigger manager.
- Add parallel download manager with resume.
- Integrate with Ferrite GUI for app store workflows.

---
*This document is a living architecture spec. It will evolve as mpkg is implemented.*
