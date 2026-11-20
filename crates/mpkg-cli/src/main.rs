use clap::{Arg, Command};
use colored::Colorize;
use mpkg_core::{PackageManifest, ResolveOptions, Resolver};
use semver::Version;
use std::collections::BTreeMap;
use std::io::{self, Write};

fn main() {
    let matches = Command::new("mpkg")
        .version("0.4.0")
        .about("mpkg — modern package manager for Ferrite/CraftOS")
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output machine-readable JSON")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("install")
                .about("Install one or more packages")
                .arg(
                    Arg::new("packages")
                        .required(true)
                        .num_args(1..)
                        .help("Package name(s) to install, optionally with constraints (e.g., pkg>=1.2.0)"),
                ),
        )
        .subcommand(
            Command::new("upgrade")
                .about("Upgrade installed packages")
                .arg(
                    Arg::new("packages")
                        .num_args(0..)
                        .help("Package names to upgrade; upgrade all if omitted"),
                ),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove installed packages")
                .arg(
                    Arg::new("packages")
                        .required(true)
                        .num_args(1..)
                        .help("Package names to remove"),
                ),
        )
        .subcommand(
            Command::new("autoremove")
                .about("List orphaned packages")
                .arg(
                    Arg::new("remove")
                        .long("remove")
                        .action(clap::ArgAction::SetTrue)
                        .help("Remove orphaned packages"),
                ),
        )
        .subcommand(
            Command::new("lock")
                .about("Write lockfile of currently resolved set"),
        )
        .subcommand(
            Command::new("list")
                .about("List installed packages")
                .arg(
                    Arg::new("upgradable")
                        .long("upgradable")
                        .action(clap::ArgAction::SetTrue)
                        .help("Show upgradable packages"),
                ),
        )
        .get_matches();

    // Prepare a simple repo and installed set for demo purposes
    // In real usage, these come from Ferrite/CraftOS
    let mut installed: BTreeMap<String, Version> = BTreeMap::new();
    let mut repo_map: BTreeMap<String, Vec<PackageManifest>> = BTreeMap::new();

    // Mock manifests (real builds load these from disk)
    let zsh = PackageManifest {
        name: "zsh".to_string(),
        version: "5.9.0".to_string(),
        description: None,
        license: None,
        depends: vec!["libiconv>=1.16".to_string()],
        provides: vec![],
    };
    let libiconv = PackageManifest {
        name: "libiconv".to_string(),
        version: "1.16.0".to_string(),
        description: None,
        license: None,
        depends: vec![],
        provides: vec![],
    };
    let git = PackageManifest {
        name: "git".to_string(),
        version: "2.43.0".to_string(),
        description: None,
        license: None,
        depends: vec!["zlib>=1.2.13".to_string()],
        provides: vec![],
    };
    let zlib = PackageManifest {
        name: "zlib".to_string(),
        version: "1.2.13".to_string(),
        description: None,
        license: None,
        depends: vec![],
        provides: vec![],
    };

    // Populate repo with multiple versions for upgrade demo
    let mut git_v242 = git.clone();
    git_v242.version = "2.42.0".to_string();
    let mut git_v241 = git.clone();
    git_v241.version = "2.41.0".to_string();

    repo_map.insert("zsh".into(), vec![zsh.clone()]);
    repo_map.insert("libiconv".into(), vec![libiconv.clone()]);
    repo_map.insert("git".into(), vec![git_v241.clone(), git_v242.clone(), git.clone()]);
    repo_map.insert("zlib".into(), vec![zlib.clone()]);

    // Simulate installed state
    installed.insert("zsh".into(), Version::parse("5.8.0").unwrap());
    installed.insert("libiconv".into(), Version::parse("1.15.0").unwrap());
    installed.insert("git".into(), Version::parse("2.42.0").unwrap());

    let json_mode = matches.get_flag("json");

    match matches.subcommand() {
        Some(("install", sub_matches)) => {
            let specs: Vec<String> = sub_matches
                .get_many::<String>("packages")
                .unwrap()
                .cloned()
                .collect();
            execute_install(&installed, &repo_map, &specs, json_mode);
        }
        Some(("upgrade", sub_matches)) => {
            let specs: Vec<String> = sub_matches
                .get_many::<String>("packages")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            execute_upgrade(&installed, &repo_map, &specs, json_mode);
        }
        Some(("remove", sub_matches)) => {
            let specs: Vec<String> = sub_matches
                .get_many::<String>("packages")
                .unwrap()
                .cloned()
                .collect();
            execute_remove(&installed, &repo_map, &specs, json_mode);
        }
        Some(("autoremove", sub_matches)) => {
            execute_autoremove(&installed, &repo_map, sub_matches.get_flag("remove"), json_mode);
        }
        Some(("lock", _)) => {
            execute_lock(&installed, &repo_map, json_mode);
        }
        Some(("list", sub_matches)) => {
            execute_list(&installed, &repo_map, sub_matches.get_flag("upgradable"), json_mode);
        }
        _ => {
            let _ = writeln!(
                io::stderr(),
                "{} Use --help to see available commands.",
                "error:".red().bold()
            );
            std::process::exit(1);
        }
    }
}

fn execute_install(
    installed: &BTreeMap<String, Version>,
    repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    specs: &[String],
    json: bool,
) {
    let resolver = Resolver::new(installed.clone(), repo_map.clone());
    match resolver.resolve(specs.iter().cloned(), ResolveOptions { upgrade_all: false }) {
        Ok(resolved) => {
            if json {
                println!("{}", serde_json::to_string(&resolved).unwrap());
            } else {
                println!("{}:", "Resolved".green().bold());
                for (name, ver) in &resolved {
                    println!("  {} {}", name.green(), ver.green());
                }
                println!("Total: {} package(s)", resolved.len().to_string().green());
            }
        }
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                let _ = writeln!(io::stderr(), "{}", e.to_string().red());
            }
            std::process::exit(1);
        }
    }
}

fn execute_upgrade(
    installed: &BTreeMap<String, Version>,
    repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    specs: &[String],
    json: bool,
) {
    let resolver = Resolver::new(installed.clone(), repo_map.clone());
    let opts = ResolveOptions { upgrade_all: specs.is_empty() };
    match resolver.resolve(specs.iter().cloned(), opts) {
        Ok(resolved) => {
            if json {
                println!("{}", serde_json::to_string(&resolved).unwrap());
            } else {
                println!("{}:", "Upgraded".yellow().bold());
                for (name, ver) in &resolved {
                    println!("  {} {}", name.yellow(), ver.yellow());
                }
                println!("Total: {} package(s)", resolved.len().to_string().yellow());
            }
        }
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                let _ = writeln!(io::stderr(), "{}", e.to_string().red());
            }
            std::process::exit(1);
        }
    }
}

fn execute_remove(
    _installed: &BTreeMap<String, Version>,
    _repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    specs: &[String],
    json: bool,
) {
    if json {
        println!("{}", serde_json::json!({"removed": specs}));
    } else {
        println!("{}:", "Removing".red().bold());
        for name in specs {
            println!("  {}", name.red());
        }
        println!("Total: {} package(s)", specs.len().to_string().red());
    }
}

fn execute_autoremove(
    installed: &BTreeMap<String, Version>,
    repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    remove: bool,
    json: bool,
) {
    let resolver = Resolver::new(installed.clone(), repo_map.clone());
    let orphans = resolver.detect_orphans(installed);
    if json {
        println!("{}", serde_json::to_string(&orphans).unwrap());
    } else {
        if orphans.is_empty() {
            println!("{} no orphans", "Autoremove:".bold());
        } else {
            println!("{}:", "Orphans".bold());
            for (name, ver) in &orphans {
                println!("  {} {}", name.red(), ver.to_string().red());
            }
            if remove {
                println!("\nRemoving listed packages...");
                // Simulate removal
            }
        }
    }
}

fn execute_lock(
    installed: &BTreeMap<String, Version>,
    _repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    json: bool,
) {
    let out = Resolver::generate_lockfile(
        &installed
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect::<Vec<_>>(),
    );
    if json {
        println!("{}", serde_json::json!({"lockfile": out}));
    } else {
        println!("{}", out);
    }
}

fn execute_list(
    installed: &BTreeMap<String, Version>,
    repo_map: &BTreeMap<String, Vec<PackageManifest>>,
    upgradable: bool,
    json: bool,
) {
    let mut upgradable_list = Vec::new();
    for (name, ver) in installed {
        if let Some(candidates) = repo_map.get(name) {
            if let Some(best) = candidates.iter().map(|m| Version::parse(&m.version).unwrap()).max() {
                if best > *ver {
                    upgradable_list.push((name.clone(), ver.clone(), best));
                }
            }
        }
    }

    if json {
        let list: Vec<_> = installed
            .iter()
            .map(|(k, v)| format!("{} {}", k, v))
            .collect();
        println!("{}", serde_json::json!({"installed": list}));
        if upgradable {
            let upgrades: Vec<_> = upgradable_list
                .iter()
                .map(|(n, c, b)| format!("{} {} -> {}", n, c, b))
                .collect();
            println!("{}", serde_json::json!({"upgradable": upgrades}));
        }
    } else {
        if upgradable {
            if upgradable_list.is_empty() {
                println!("{} no upgradable packages", "List:".bold());
            } else {
                println!("{}:", "Upgradable".yellow().bold());
                for (name, curr, best) in &upgradable_list {
                    println!("  {} {} -> {}", name.yellow(), curr.to_string().yellow(), best.to_string().yellow());
                }
            }
        } else {
            println!("{}:", "Installed".green().bold());
            for (name, ver) in installed {
                println!("  {} {}", name.green(), ver.to_string().green());
            }
            println!("Total: {} package(s)", installed.len().to_string().green());
        }
    }
}
