use anyhow::Context;
use clap::{Parser, Subcommand};
use mpkg_core::{FsBackend, LockFile, Repository};
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
            // Stub: list packages in packages_dir
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
        }
    }

    Ok(())
}