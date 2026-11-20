use anyhow::Context;
use clap::{Parser, Subcommand};
use mpkg_core::{FsBackend, RepositoryClient};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mpkg")]
#[command(about = "Rust-native package manager for Ferrite/CraftOS", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Set repository root (default: /var/lib/mpkg)
    #[arg(long, global = true, default_value = "/var/lib/mpkg")]
    repo_root: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Install one or more packages
    Install { names: Vec<String> },
    /// List installed packages
    List,
    /// Fetch and verify latest repository index
    Update,
    /// Manage trusted repository keys
    #[command(subcommand)]
    Key(KeyCommands),
    /// Search repository for packages
    Search { query: Option<String> },
    /// Show detailed information about a package
    Info { name: String },
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
                eprintln!("No packages specified");
                std::process::exit(1);
            }
            // Resolve (stub) and install
            let lock = mpkg_core::resolve_install(&backend, &names)
                .await
                .context("failed to resolve packages")?;
            mpkg_core::install_lock(&backend, &lock)
                .await
                .context("failed to install packages")?;
            println!("Installed:");
            for (n, v) in &lock.resolved {
                println!("  {} v{}", n, v);
            }
        }
        Commands::List => {
            list_installed(&backend).await?;
        }
        Commands::Update => {
            update_repo(&backend).await?;
        }
        Commands::Key(cmd) => match cmd {
            KeyCommands::Add { key_file } => {
                key_add(&backend, &key_file).await?;
            }
            KeyCommands::List => {
                key_list(&backend).await?;
            }
        },
        Commands::Search { query } => {
            search_repo(&backend, query.as_deref()).await?;
        }
        Commands::Info { name } => {
            info_repo(&backend, &name).await?;
        }
    }

    Ok(())
}

async fn list_installed(backend: &FsBackend) -> anyhow::Result<()> {
    let pkgs_dir = backend.packages_dir();
    if !pkgs_dir.exists() {
        println!("No packages installed.");
        return Ok(());
    }
    let mut list = Vec::new();
    let mut entries = tokio::fs::read_dir(pkgs_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                list.push(name.to_string());
            }
        }
    }
    if list.is_empty() {
        println!("No packages installed.");
    } else {
        list.sort();
        println!("Installed packages:");
        for n in list {
            println!("  {}", n);
        }
    }
    Ok(())
}

async fn update_repo(backend: &FsBackend) -> anyhow::Result<()> {
    let trusted_keys = load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;

    let base_url = std::env::var("MPKG_REPO_URL")
        .unwrap_or_else(|_| "https://packages.rustos.dev/v1".to_string());

    let client = RepositoryClient::new(base_url, backend.repos_dir());
    match client.update(&trusted_keys).await {
        Ok(idx) => {
            println!(
                "Repository index updated. {} packages available.",
                idx.packages.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to update repository: {}", e);
            std::process::exit(1);
        }
    }
}

async fn load_trusted_keys(dir: &std::path::Path) -> anyhow::Result<Vec<ed25519_dalek::VerifyingKey>> {
    use std::fs::read_to_string;

    let mut keys = Vec::new();
    if !dir.exists() {
        return Ok(keys);
    }

    for entry in walkdir::WalkDir::new(dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let hex_fp = match entry.file_name().to_str() {
            Some(s) if s.ends_with(".pub") => &s[..s.len() - 4],
            _ => continue,
        };
        let content = read_to_string(entry.path())?;
        let bytes = hex::decode(content.trim())?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into()?)?;
        keys.push(key);
    }

    Ok(keys)
}

async fn key_add(backend: &FsBackend, key_file: &std::path::Path) -> anyhow::Result<()> {
    use std::fs::{create_dir_all, read_to_string};

    let dest_dir = backend.repos_dir().join("trusted-keys");
    create_dir_all(&dest_dir)?;

    let content = read_to_string(key_file)?;
    let bytes = hex::decode(content.trim())?;
    let _key = ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into()?)?;

    let fp = hex::encode(bytes);
    let out_path = dest_dir.join(format!("{}.pub", fp));

    tokio::fs::write(&out_path, content).await?;
    println!("Added trusted key: {}", out_path.display());
    Ok(())
}

async fn key_list(backend: &FsBackend) -> anyhow::Result<()> {
    let dir = backend.repos_dir().join("trusted-keys");
    if !dir.exists() {
        println!("No trusted keys.");
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
    if keys.is_empty() {
        println!("No trusted keys.");
    } else {
        println!("Trusted keys:");
        for k in keys {
            println!("  {}", k);
        }
    }
    Ok(())
}

async fn search_repo(backend: &FsBackend, query: Option<&str>) -> anyhow::Result<()> {
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        println!("No local index. Run `mpkg update` first.");
        return Ok(());
    }
    let trusted_keys = load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;
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
        println!("No packages matched.");
    } else {
        println!("Packages:");
        for e in matches {
            println!("  {} v{} - {}", e.name, e.version, e.description.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

async fn info_repo(backend: &FsBackend, name: &str) -> anyhow::Result<()> {
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        println!("No local index. Run `mpkg update` first.");
        return Ok(());
    }
    let trusted_keys = load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await?;
    let idx = mpkg_core::RepoIndex::load_and_verify(&idx_path, &trusted_keys)
        .await
        .context("failed to load/verify index")?;

    if let Some(entry) = idx.packages.get(name) {
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
        println!("Package '{}' not found.", name);
    }
    Ok(())