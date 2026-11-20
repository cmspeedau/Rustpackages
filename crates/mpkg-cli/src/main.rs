use anyhow::Context;
use clap::{Parser, Subcommand};
use mpkg_core::{FsBackend, ResolveOptions};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mpkg")]
#[command(about = "Rust-native package manager for Ferrite/CraftOS", long_about = None, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Set repository root (default: /var/lib/mpkg)
    #[arg(long, global = true, default_value = "/var/lib/mpkg")]
    repo_root: PathBuf,

    /// Output machine-readable JSON (for GUI embedding)
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Install one or more packages
    Install { names: Vec<String> },
    /// List installed packages
    List,
    /// Fetch and verify latest repository index
    Update,
    /// Generate a lockfile from current installed set (upgrade policy)
    Lock,
    /// Upgrade installed packages
    Upgrade { names: Vec<String> },
    /// Remove orphaned packages no longer required
    Autoremove,
    /// Search repository for packages
    Search { query: Option<String> },
    /// Show detailed information about a package
    Info { name: String },
    /// Manage trusted repository keys
    #[command(subcommand)]
    Key(KeyCommands),
}

#[derive(Subcommand)]
enum KeyCommands {
    /// Add a trusted ed25519 public key from a file (hex fingerprint as filename)
    Add { key_file: PathBuf },
    /// List trusted keys
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let backend = FsBackend::new(&cli.repo_root);
    backend.ensure_repos_dir().await?;

    match cli.command {
        Commands::Install { names } => {
            if names.is_empty() {
                return Err(anyhow::anyhow!("No packages specified"));
            }
            let lock = mpkg_core::resolve_install(&backend, &names)
                .await
                .context("failed to resolve packages")?;

            // Human output
            if !cli.json {
                println!("Resolved to install:");
                for (n, v) in &lock.resolved {
                    println!("  {} v{}", n, v);
                }
            } else {
                #[derive(serde::Serialize)]
                struct Out { resolved: Vec<(String, String)> }
                println!("{}", serde_json::to_string(&Out { resolved: lock.resolved.clone() })?);
            }

            mpkg_core::install_lock(&backend, &lock)
                .await
                .context("failed to install packages")?;

            mpkg_core::write_lockfile(&backend, &lock)
                .await
                .context("failed to write lockfile")?;

            if !cli.json {
                println!("\x1b[32mInstalled successfully:\x1b[0m");
                for (n, v) in &lock.resolved {
                    println!("  {} v{}", n, v);
                }
            }
        }
        Commands::List => {
            list_installed(&backend, cli.json).await?;
        }
        Commands::Update => {
            update_repo(&backend, cli.json).await?;
        }
        Commands::Lock => {
            let lock = mpkg_core::resolve_upgrade(&backend).await
                .context("failed to resolve upgrade set")?;

            if !cli.json {
                println!("Lockfile will contain:");
                for (n, v) in &lock.resolved {
                    println!("  {} v{}", n, v);
                }
            } else {
                #[derive(serde::Serialize)]
                struct Out { resolved: Vec<(String, String)> }
                println!("{}", serde_json::to_string(&Out { resolved: lock.resolved.clone() })?);
            }

            mpkg_core::write_lockfile(&backend, &lock)
                .await
                .context("failed to write lockfile")?;

            if !cli.json {
                println!("Lockfile written to {}", backend.lockfile_path().display());
            }
        }
        Commands::Upgrade { names } => {
            let mut opts = ResolveOptions::default();
            if names.is_empty() {
                opts.upgrade_all = true;
            }
            let lock = mpkg_core::resolve_install(&backend, &names)
                .await
                .context("failed to resolve upgrade")?;

            if lock.resolved.is_empty() {
                if !cli.json {
                    println!("All packages are up to date.");
                } else {
                    #[derive(serde::Serialize)]
                    struct Out { upgraded: Vec<(String, String)> }
                    println!("{}", serde_json::to_string(&Out { upgraded: vec![] })?);
                }
            } else {
                mpkg_core::install_lock(&backend, &lock)
                    .await
                    .context("failed to install upgraded packages")?;

                mpkg_core::write_lockfile(&backend, &lock)
                    .await
                    .context("failed to write lockfile")?;

                if !cli.json {
                    println!("\x1b[33mUpgraded:\x1b[0m");
                    for (n, v) in &lock.resolved {
                        println!("  {} v{}", n, v);
                    }
                } else {
                    #[derive(serde::Serialize)]
                    struct Out { upgraded: Vec<(String, String)> }
                    println!("{}", serde_json::to_string(&Out { upgraded: lock.resolved.clone() })?);
                }
            }
        }
        Commands::Autoremove => {
            let orphans = mpkg_core::autoremove(&backend).await
                .context("failed to detect orphans")?;

            if orphans.is_empty() {
                if !cli.json {
                    println!("No orphaned packages.");
                } else {
                    #[derive(serde::Serialize)]
                    struct Out { orphans: Vec<(String, String)> }
                    println!("{}", serde_json::to_string(&Out { orphans: vec![] })?);
                }
            } else {
                if !cli.json {
                    println!("\x1b[31mOrphaned packages to remove:\x1b[0m");
                    for (n, v) in &orphans {
                        println!("  {} v{}", n, v);
                    }
                    // Stub: do not actually remove; user must confirm
                } else {
                    #[derive(serde::Serialize)]
                    struct Out { orphans: Vec<(String, String)> }
                    println!("{}", serde_json::to_string(&Out { orphans })?);
                }
            }
        }
        Commands::Key(cmd) => match cmd {
            KeyCommands::Add { key_file } => {
                key_add(&backend, &key_file, cli.json).await?;
            }
            KeyCommands::List => {
                key_list(&backend, cli.json).await?;
            }
        },
        Commands::Search { query } => {
            search_repo(&backend, query.as_deref(), cli.json).await?;
        }
        Commands::Info { name } => {
            info_repo(&backend, &name, cli.json).await?;
        }
    }

    Ok(())
}

async fn list_installed(backend: &FsBackend, json: bool) -> anyhow::Result<()> {
    let list = mpkg_core::list_installed_packages(backend).await?;
    if list.is_empty() {
        if !json {
            println!("No packages installed.");
        } else {
            #[derive(serde::Serialize)]
            struct Out { installed: Vec<(String, String)> }
            println!("{}", serde_json::to_string(&Out { installed: vec![] })?);
        }
    } else {
        if !json {
            println!("Installed packages:");
            for (n, v) in &list {
                println!("  {} v{}", n, v);
            }
        } else {
            #[derive(serde::Serialize)]
            struct Out { installed: Vec<(String, String)> }
            println!("{}", serde_json::to_string(&Out { installed: list.clone() })?);
        }
    }
    Ok(())
}

async fn update_repo(backend: &FsBackend, json: bool) -> anyhow::Result<()> {
    let trusted_keys = mpkg_core::repo::load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;

    let base_url = std::env::var("MPKG_REPO_URL").unwrap_or_else(|_| "https://packages.rustos.dev/v1".to_string());

    let client = mpkg_core::RepositoryClient::new(base_url, backend.repos_dir());
    match client.update(&trusted_keys).await {
        Ok(idx) => {
            if !json {
                println!("Repository index updated. {} packages available.", idx.packages.len());
            } else {
                #[derive(serde::Serialize)]
                struct Out { packages: usize }
                println!("{}", serde_json::to_string(&Out { packages: idx.packages.len() })?);
            }
            Ok(())
        }
        Err(e) => {
            if !json {
                eprintln!("Failed to update repository: {}", e);
            } else {
                #[derive(serde::Serialize)]
                struct ErrOut { error: String }
                println!("{}", serde_json::to_string(&ErrOut { error: e.to_string() })?);
            }
            std::process::exit(1);
        }
    }
}

async fn key_add(backend: &FsBackend, key_file: &std::path::Path, json: bool) -> anyhow::Result<()> {
    use std::fs::{create_dir_all, read_to_string};

    let dest_dir = backend.repos_dir().join("trusted-keys");
    create_dir_all(&dest_dir)?;

    let content = read_to_string(key_file)?;
    let bytes = hex::decode(content.trim())?;
    let _key = ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into()?)?;

    let fp = hex::encode(bytes);
    let out_path = dest_dir.join(format!("{}.pub", fp));

    tokio::fs::write(&out_path, &content).await?;

    if !json {
        println!("Added trusted key: {}", out_path.display());
    } else {
        #[derive(serde::Serialize)]
        struct Out { key_file: String }
        println!("{}", serde_json::to_string(&Out { key_file: out_path.display().to_string() })?);
    }
    Ok(())
}

async fn key_list(backend: &FsBackend, json: bool) -> anyhow::Result<()> {
    let dir = backend.repos_dir().join("trusted-keys");
    if !dir.exists() {
        if !json {
            println!("No trusted keys.");
        } else {
            #[derive(serde::Serialize)]
            struct Out { keys: Vec<String> }
            println!("{}", serde_json::to_string(&Out { keys: vec![] })?);
        }
        return Ok(());
    }
    let mut keys = Vec::new();
    for entry in walkdir::WalkDir::new(&dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && entry.path().extension().map(|e| e == "pub").unwrap_or(false) {
            if let Some(name) = entry.file_name().to_str() {
                keys.push(name.to_string());
            }
        }
    }
    if !json {
        if keys.is_empty() {
            println!("No trusted keys.");
        } else {
            println!("Trusted keys:");
            for k in &keys {
                println!("  {}", k);
            }
        }
    } else {
        #[derive(serde::Serialize)]
        struct Out { keys: Vec<String> }
        println!("{}", serde_json::to_string(&Out { keys })?);
    }
    Ok(())
}

async fn search_repo(backend: &FsBackend, query: Option<&str>, json: bool) -> anyhow::Result<()> {
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        if !json {
            println!("No local index. Run `mpkg update` first.");
        } else {
            #[derive(serde::Serialize)]
            struct ErrOut { error: String }
            println!("{}", serde_json::to_string(&ErrOut { error: "no local index; run `mpkg update` first".into() })?);
        }
        return Ok(());
    }
    let trusted_keys = mpkg_core::repo::load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;
    let idx = mpkg_core::RepoIndex::load_and_verify(&idx_path, &trusted_keys)
        .await
        .context("failed to load/verify index")?;

    let q = query.unwrap_or("");
    let mut matches: Vec<_> = idx
        .packages
        .values()
        .filter(|e| e.name.contains(q) || q.is_empty())
        .collect();
    matches.sort_by(|a, b| a.name.cmp(&b.name));

    if matches.is_empty() {
        if !json {
            println!("No packages matched.");
        } else {
            #[derive(serde::Serialize)]
            struct Out { packages: Vec<String> }
            println!("{}", serde_json::to_string(&Out { packages: vec![] })?);
        }
    } else {
        if !json {
            println!("Packages:");
            for e in &matches {
                println!(
                    "  {} v{} - {}",
                    e.name,
                    e.version,
                    e.description.as_deref().unwrap_or("")
                );
            }
        } else {
            #[derive(serde::Serialize)]
            struct Out { packages: Vec<(String, String, Option<String>)> }
            let out: Vec<_> = matches.iter().map(|e| (e.name.clone(), e.version.clone(), e.description.clone())).collect();
            println!("{}", serde_json::to_string(&Out { packages: out })?);
        }
    }
    Ok(())
}

async fn info_repo(backend: &FsBackend, name: &str, json: bool) -> anyhow::Result<()> {
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        if !json {
            println!("No local index. Run `mpkg update` first.");
        } else {
            #[derive(serde::Serialize)]
            struct ErrOut { error: String }
            println!("{}", serde_json::to_string(&ErrOut { error: "no local index; run `mpkg update` first".into() })?);
        }
        return Ok(());
    }
    let trusted_keys = mpkg_core::repo::load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;
    let idx = mpkg_core::RepoIndex::load_and_verify(&idx_path, &trusted_keys)
        .await
        .context("failed to load/verify index")?;

    if let Some(entry) = idx.packages.get(name) {
        if !json {
            println!("Package: {}", entry.name);
            println!("Version: {}", entry.version);
            println!("Archive: {}", entry.archive);
            println!("Size: {} bytes", entry.size);
            println!("SHA256: {}", entry.sha256);
            println!("Deps: {}", entry.deps.join(", "));
            if !entry.manifest.is_empty() {
                println!("Manifest:\n{}", entry.manifest);
            }
        } else {
            #[derive(serde::Serialize)]
            struct Out {
                name: String,
                version: String,
                archive: String,
                size: u64,
                sha256: String,
                deps: Vec<String>,
                manifest: String,
            }
            println!("{}", serde_json::to_string(&Out {
                name: entry.name.clone(),
                version: entry.version.clone(),
                archive: entry.archive.clone(),
                size: entry.size,
                sha256: entry.sha256.clone(),
                deps: entry.deps.clone(),
                manifest: entry.manifest.clone(),
            })?);
        }
    } else {
        if !json {
            println!("Package '{}' not found.", name);
        } else {
            #[derive(serde::Serialize)]
            struct ErrOut { error: String }
            println!("{}", serde_json::to_string(&ErrOut { error: format!("package '{}' not found", name) })?);
        }
    }
    Ok(())
}
