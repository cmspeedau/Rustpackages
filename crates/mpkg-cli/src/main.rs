use anyhow::Context;
use clap::{Parser, Subcommand};
use mpkg_core::repo::{RepoIndex, RepositoryClient};
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
    /// Fetch and verify the latest repository index
    Update,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { names } => {
            // TODO Phase 3: real resolution and install
            if names.is_empty() {
                eprintln!("No packages specified");
                std::process::exit(1);
            }
            println!("Would install: {:?}", names);
        }
        Commands::List => {
            println!("List installed packages (stub)");
        }
        Commands::Update => {
            let trusted_keys = load_trusted_keys("/etc/mpkg/trusted-keys").await?;
            let base_url = std::env::var("MPKG_REPO_URL")
                .unwrap_or_else(|_| "https://packages.rustos.dev/v1".to_string());
            let client = RepositoryClient::new(base_url, mpkg_core::local_cache_dir().await);
            let idx = client.update(&trusted_keys).await.map_err(|e| anyhow::anyhow!(e))?;
            println!("Repository index updated. {} packages available.", idx.packages.len());
        }
    }

    Ok(())
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
        let content = read_to_string(entry.path()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let bytes = hex::decode(content.trim()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into().map_err(|_| anyhow::anyhow!("bad key length"))?)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        keys.push(key);
    }

    Ok(keys)
}