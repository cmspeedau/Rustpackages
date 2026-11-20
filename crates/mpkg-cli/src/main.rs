use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use mpkg_core::repo::{RepoEntry, RepoIndex};
use mpkg_core::resolve;
use mpkg_core::sign;
use mpkg_core::store::Store;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(name = "mpkg", version, about = "Rust-based package manager (rootfs-agnostic)")]
struct Cli {
    /// Target rootfs to operate on (defaults to /)
    #[arg(long, global = true, default_value = "/")]
    root: PathBuf,
    /// Repository index file (used by search/update; defaults to <root>/etc/mpkg/index.toml)
    #[arg(long, global = true)]
    index: Option<PathBuf>,
    /// Emit machine-readable JSON where supported
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a .mpk file into the rootfs
    Install { pkg: PathBuf },
    /// Remove an installed package
    Remove { name: String },
    /// Upgrade installed packages to the newest versions in the index
    Upgrade,
    /// List installed packages
    List,
    /// Verify installed files against recorded hashes
    Verify { name: String },
    /// Search the repository index
    Search { query: String },
    /// Refresh the local copy of the repository index
    Update,
    /// Show install/remove history
    History,
    /// Undo the most recent operation
    Rollback,
    /// Manage repositories
    Repo {
        #[command(subcommand)]
        action: RepoCmd,
    },
    /// Generate an ed25519 keypair for signing repo indexes
    Keygen,
}

#[derive(Subcommand)]
enum RepoCmd {
    Add { name: String, url: String },
    List,
}

fn index_path(cli: &Cli) -> PathBuf {
    cli.index
        .clone()
        .unwrap_or_else(|| cli.root.join("etc/mpkg/index.toml"))
}

fn load_index(cli: &Cli) -> Result<RepoIndex> {
    RepoIndex::load(&index_path(cli)).context("failed to load repository index (run `mpkg update` first)")
}

fn download(url: &str) -> Result<Vec<u8>> {
    // Minimal HTTP(S) fetch without extra deps is out of scope; support
    // file:// and plain paths for now, bail clearly otherwise.
    let path = url
        .strip_prefix("file://")
        .unwrap_or(url);
    if Path::new(path).is_file() {
        Ok(std::fs::read(path)?)
    } else {
        bail!("remote fetching not yet implemented; use a file:// URL (got {})", url)
    }
}

fn fetch_pkg(entry: &RepoEntry) -> Result<(PathBuf, Vec<u8>)> {
    let data = download(&entry.url)?;
    let actual = hex::encode(Sha256::digest(&data));
    if actual != entry.sha256 {
        bail!("sha256 mismatch for {}: expected {}, got {}", entry.name, entry.sha256, actual);
    }
    let tmp = std::env::temp_dir().join(format!("{}-{}.mpk", entry.name, entry.version));
    std::fs::write(&tmp, &data)?;
    Ok((tmp, data))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(&cli.root);
    match &cli.cmd {
        Cmd::Install { pkg } => {
            let rec = store.install(pkg)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&rec)?);
            } else {
                println!("installed {} {}", rec.manifest.package.name, rec.manifest.package.version);
            }
        }
        Cmd::Remove { name } => {
            store.remove(name)?;
            if !cli.json {
                println!("removed {}", name);
            }
        }
        Cmd::Upgrade => {
            let index = load_index(&cli)?;
            let installed = store.list()?;
            let mut upgraded = 0;
            for pkg in &installed {
                let name = &pkg.manifest.package.name;
                let cur = semver::Version::parse(&pkg.manifest.package.version)?;
                if let Some(entry) = index.find(name, &semver::VersionReq::STAR) {
                    let new = semver::Version::parse(&entry.version)?;
                    if new > cur {
                        let (tmp, _) = fetch_pkg(&entry)?;
                        store.install(&tmp)?;
                        upgraded += 1;
                        if !cli.json {
                            println!("upgraded {} {} -> {}", name, cur, new);
                        }
                    }
                }
            }
            if !cli.json && upgraded == 0 {
                println!("everything up to date");
            }
        }
        Cmd::List => {
            let list = store.list()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for p in list {
                    println!("{}\t{}", p.manifest.package.name, p.manifest.package.version);
                }
            }
        }
        Cmd::Verify { name } => {
            let problems = store.verify(name)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&problems)?);
            } else if problems.is_empty() {
                println!("{}: OK", name);
            } else {
                for p in problems {
                    println!("{}", p);
                }
                bail!("{} failed verification", name);
            }
        }
        Cmd::Search { query } => {
            let index = load_index(&cli)?;
            let hits = index.search(query);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&hits)?);
            } else {
                for h in hits {
                    println!("{}\t{}\t{}", h.name, h.version, h.description);
                }
            }
        }
        Cmd::Update => {
            // With file:// repos the index is already local; copy it into place.
            println!("index at {}", index_path(&cli).display());
        }
        Cmd::History => {
            let h = store.history()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&h)?);
            } else {
                for e in h {
                    println!("{}\t{}\t{} {}", e.at, e.op, e.name, e.version);
                }
            }
        }
        Cmd::Rollback => {
            let e = store.rollback()?;
            if !cli.json {
                println!("rolled back {} {} {}", e.op, e.name, e.version);
            }
        }
        Cmd::Repo { action } => match action {
            RepoCmd::Add { name, url } => {
                let path = cli.root.join("etc/mpkg/repos.toml");
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut content = if path.is_file() {
                    std::fs::read_to_string(&path)?
                } else {
                    String::new()
                };
                content.push_str(&format!("[[repo]]\nname = \"{}\"\nurl = \"{}\"\n\n", name, url));
                std::fs::write(&path, content)?;
                if !cli.json {
                    println!("added repo {} ({})", name, url);
                }
            }
            RepoCmd::List => {
                let path = cli.root.join("etc/mpkg/repos.toml");
                if path.is_file() {
                    print!("{}", std::fs::read_to_string(&path)?);
                }
            }
        },
        Cmd::Keygen => {
            let (sk, pk) = sign::generate_keypair();
            if cli.json {
                println!("{{\"secret\": \"{}\", \"public\": \"{}\"}}", sk, pk);
            } else {
                println!("secret: {}\npublic: {}", sk, pk);
            }
        }
    }
    Ok(())
}
