//! Command-line interface for the Rustpackages package manager.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mpkg_core::lockfile::Lockfile;
use mpkg_core::remote;
use mpkg_core::repo::RepoIndex;
use mpkg_core::store::Store;

#[derive(Parser)]
#[command(name = "mpkg", version, about = "Rustpackages package manager")]
struct Cli {
    /// Target rootfs (defaults to /).
    #[arg(long, global = true, default_value = "/")]
    root: PathBuf,
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install a local .mpk archive (or download from a repo URL).
    Install { package: String },
    /// Remove an installed package.
    Remove { name: String },
    /// Upgrade installed packages to the newest versions in a repo.
    Upgrade { repo: Option<String> },
    /// List installed packages.
    List,
    /// Verify installed files against recorded hashes.
    Verify { name: Option<String> },
    /// Search a repository index.
    Search { repo: String, query: String },
    /// Fetch and cache a remote repo index.
    Update { repo: String },
    /// Show operation history.
    History,
    /// Undo the last operation.
    Rollback,
    /// Manage repositories.
    Repo {
        #[command(subcommand)]
        action: RepoCommand,
    },
    /// Generate an ed25519 signing keypair.
    Keygen { name: String },
}

#[derive(Subcommand)]
enum RepoCommand {
    Add { name: String, url: String },
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> mpkg_core::Result<()> {
    let store = Store::new(&cli.root);
    match cli.cmd {
        Command::Install { package } => {
            let path = if package.starts_with("http://") || package.starts_with("https://") || package.starts_with("file://") {
                let tmp = cli.root.join("var/lib/mpkg/cache");
                let name_ver = package.rsplit('/').next().unwrap_or("pkg.mpk").to_string();
                let (name, version) = name_ver
                    .trim_end_matches(".mpk")
                    .rsplit_once('-')
                    .map(|(n, v)| (n.to_string(), v.to_string()))
                    .unwrap_or((name_ver.clone(), String::new()));
                remote::fetch_package(&package[..package.rfind('/').unwrap_or(0)], &name, &version, &tmp)?
            } else {
                PathBuf::from(&package)
            };
            let rec = store.install(&path)?;
            if cli.json {
                println!("{}", serde_json::to_string(&rec).unwrap());
            } else {
                println!("installed {} {}", rec.manifest.name, rec.manifest.version);
            }
        }
        Command::Remove { name } => {
            let rec = store.remove(&name)?;
            println!("removed {} {}", rec.manifest.name, rec.manifest.version);
        }
        Command::Upgrade { repo } => {
            let repo_url = repo.ok_or_else(|| mpkg_core::MpkgError::Parse("upgrade requires a repo URL".into()))?;
            let index = remote::fetch_index(&repo_url)?;
            let installed = store.list()?;
            let mut upgraded = 0;
            for pkg in installed {
                if let Some(latest) = index.find(&pkg.manifest.name) {
                    if latest.version != pkg.manifest.version {
                        let tmp = cli.root.join("var/lib/mpkg/cache");
                        let mpk = remote::fetch_package(&repo_url, &latest.name, &latest.version, &tmp)?;
                        store.remove(&pkg.manifest.name)?;
                        store.install(&mpk)?;
                        upgraded += 1;
                        println!("upgraded {} {} -> {}", latest.name, pkg.manifest.version, latest.version);
                    }
                }
            }
            if upgraded == 0 {
                println!("everything up to date");
            }
        }
        Command::List => {
            let pkgs = store.list()?;
            if cli.json {
                println!("{}", serde_json::to_string(&pkgs).unwrap());
            } else {
                for p in pkgs {
                    println!("{} {}", p.manifest.name, p.manifest.version);
                }
            }
        }
        Command::Verify { name } => {
            let names: Vec<String> = match name {
                Some(n) => vec![n],
                None => store.list()?.iter().map(|p| p.manifest.name.clone()).collect(),
            };
            let mut bad = 0;
            for n in names {
                let mismatches = store.verify(&n)?;
                if mismatches.is_empty() {
                    println!("{n}: OK");
                } else {
                    bad += 1;
                    println!("{n}: {} file(s) modified/missing", mismatches.len());
                }
            }
            if bad > 0 {
                return Err(mpkg_core::MpkgError::VerifyFailed(bad));
            }
        }
        Command::Search { repo, query } => {
            let index = remote::fetch_index(&repo)?;
            for p in index.search(&query) {
                println!("{} {} — {}", p.name, p.version, p.description);
            }
        }
        Command::Update { repo } => {
            let index = remote::fetch_index(&repo)?;
            let dir = cli.root.join("var/lib/mpkg/repos");
            std::fs::create_dir_all(&dir).map_err(mpkg_core::MpkgError::Io)?;
            let name = repo.rsplit('/').next().unwrap_or("repo").trim_end_matches(".git").to_string();
            index.save(&dir.join(format!("{name}.toml")))?;
            println!("updated index for {repo} ({} packages)", index.packages.len());
        }
        Command::History => {
            let path = cli.root.join("var/lib/mpkg/history.jsonl");
            match std::fs::read_to_string(&path) {
                Ok(text) => print!("{text}"),
                Err(_) => println!("(no history)"),
            }
        }
        Command::Rollback => {
            let entry = store.rollback()?;
            println!("rolled back {} {}", entry.name(), entry.version());
        }
        Command::Repo { action } => match action {
            RepoCommand::Add { name, url } => {
                let dir = cli.root.join("var/lib/mpkg/repos");
                std::fs::create_dir_all(&dir).map_err(mpkg_core::MpkgError::Io)?;
                let mut cfg: toml::Value = match std::fs::read_to_string(dir.join("repos.toml")) {
                    Ok(t) => toml::from_str(&t).unwrap_or(toml::Value::Table(Default::default())),
                    Err(_) => toml::Value::Table(Default::default()),
                };
                cfg["repos"][name.as_str()] = toml::Value::String(url);
                std::fs::write(dir.join("repos.toml"), toml::to_string_pretty(&cfg).unwrap()).map_err(mpkg_core::MpkgError::Io)?;
                println!("added repo {name} -> {url}");
            }
            RepoCommand::List => {
                let path = cli.root.join("var/lib/mpkg/repos/repos.toml");
                match std::fs::read_to_string(&path) {
                    Ok(t) => print!("{t}"),
                    Err(_) => println!("(no repos configured)"),
                }
            }
        },
        Command::Keygen { name } => {
            let dir = cli.root.join("var/lib/mpkg/keys");
            let (pk, sk) = mpkg_core::sign::generate_keypair();
            mpkg_core::sign::save_keypair(&dir, &name, &pk, &sk)?;
            println!("wrote keypair to {}", dir.join(&name).display());
        }
    }
    Ok(())
}
