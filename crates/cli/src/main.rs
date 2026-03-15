use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, Level};

#[derive(Parser)]
#[command(name = "brew-rs")]
#[command(version, about = "A blazing-fast package manager written in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize brew-rs directories and configuration
    Init,
    /// Install a package
    Install {
        /// Package name to install
        package: String,
        /// Show what would be installed without actually installing
        #[arg(long)]
        dry_run: bool,
    },
    /// Uninstall a package
    Uninstall {
        /// Package name to uninstall
        package: String,
    },
    /// Search for packages
    Search {
        /// Search query
        query: String,
    },
    /// Show package information
    Info {
        /// Package name
        package: String,
    },
    /// List installed packages
    List,
    /// Update package repositories
    Update,
    /// Upgrade installed packages
    Upgrade {
        /// Specific package to upgrade (optional)
        package: Option<String>,
    },
    /// Manage package taps (repositories)
    Tap {
        #[command(subcommand)]
        command: TapCommands,
    },
}

#[derive(Subcommand)]
enum TapCommands {
    /// Add a new tap
    Add {
        /// Tap name (e.g., brew-rs/core)
        name: String,
        /// Git repository URL
        url: String,
    },
    /// Remove a tap
    Remove {
        /// Tap name to remove
        name: String,
    },
    /// Update taps
    Update {
        /// Specific tap to update (updates all if not specified)
        name: Option<String>,
    },
    /// List installed taps
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    info!("brew-rs v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::Init => {
            info!("Initializing brew-rs");
            match brew_config::Config::load() {
                Ok(config) => {
                    match config.init_directories() {
                        Ok(_) => {
                            println!("✓ Initialized brew-rs directories:");
                            println!("  Data:    {}", config.paths.data_dir.display());
                            println!("  Config:  {}", config.paths.config_dir.display());
                            println!("  Cache:   {}", config.paths.cache_dir.display());
                            println!("  Cellar:  {}", config.paths.cellar_dir.display());
                            println!("  Bin:     {}", config.paths.bin_dir.display());

                            // Save default config if it doesn't exist
                            if !config.paths.config_file.exists() {
                                if let Err(e) = config.save() {
                                    println!("⚠ Warning: Could not save config: {}", e);
                                } else {
                                    println!("  Config file created: {}", config.paths.config_file.display());
                                }
                            }

                            // Check if bin directory is in PATH
                            if !config.paths.is_bin_in_path() {
                                println!("\n⚠ Warning: {} is not in your PATH", config.paths.bin_dir.display());
                                println!("Add this to your shell rc file (~/.zshrc or ~/.bashrc):");
                                println!("  export PATH=\"{}:$PATH\"", config.paths.bin_dir.display());
                            }
                        }
                        Err(e) => {
                            eprintln!("Error initializing directories: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Install { package, dry_run } => {
            info!("Installing package: {}", package);
            match brew_config::Config::load() {
                Ok(config) => {
                    match brew_tap::TapManager::new(config.paths.clone()) {
                        Ok(tap_manager) => {
                            match tap_manager.find_formula(&package) {
                                Ok(formula) => {
                                    // ── 1. Recursively collect all dep formulas ──────────
                                    let mut all_formulas: Vec<brew_formula::Formula> =
                                        vec![formula.clone()];
                                    let mut visited: std::collections::HashSet<String> =
                                        std::collections::HashSet::new();
                                    visited.insert(formula.name().to_string());
                                    let mut to_load: Vec<String> = formula
                                        .dependencies
                                        .runtime
                                        .iter()
                                        .map(|d| d.name.clone())
                                        .collect();
                                    while let Some(dep_name) = to_load.pop() {
                                        if !visited.insert(dep_name.clone()) {
                                            continue; // already processed — skip to prevent infinite loop
                                        }
                                        if let Ok(dep_formula) =
                                            tap_manager.find_formula(&dep_name)
                                        {
                                            for subdep in &dep_formula.dependencies.runtime {
                                                to_load.push(subdep.name.clone());
                                            }
                                            all_formulas.push(dep_formula);
                                        }
                                    }

                                    // ── 2. SAT-resolve version constraints ───────────────
                                    let mut resolver = brew_solver::Resolver::new();
                                    for f in &all_formulas {
                                        resolver.add_formula(f.clone());
                                    }

                                    let resolved = match resolver.resolve(&package) {
                                        Ok(r) => r,
                                        Err(e) => {
                                            eprintln!("Dependency resolution failed: {}", e);
                                            std::process::exit(1);
                                        }
                                    };

                                    // ── 3. Build ordered install queue ───────────────────
                                    let mut queue = brew_solver::InstallQueue::new();

                                    let installed = match brew_core::Database::open(&config.paths) {
                                        Ok(db) => db
                                            .packages()
                                            .list_all()
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|p| p.name)
                                            .collect(),
                                        Err(_) => std::collections::HashSet::new(),
                                    };
                                    queue.set_installed(installed);

                                    // Add formulas that appear in the SAT resolution
                                    let resolved_names: std::collections::HashSet<String> =
                                        resolved.iter().map(|(n, _)| n.clone()).collect();
                                    for f in &all_formulas {
                                        if !resolved_names.contains(f.name()) {
                                            continue;
                                        }
                                        if f.name() == package {
                                            let _ = queue.add_root(f.clone());
                                        } else {
                                            let _ = queue.add_dependency(f.clone());
                                        }
                                    }

                                    // ── 4. Display results ───────────────────────────────
                                    if dry_run {
                                        match queue.dry_run_summary() {
                                            Ok(summary) => {
                                                // Print version-pinned summary
                                                let resolved_map: std::collections::HashMap<
                                                    String,
                                                    semver::Version,
                                                > = resolved.into_iter().collect();

                                                // Build a map from dep name → constraint string
                                                // by scanning all formula dependency lists
                                                let mut constraint_map: std::collections::HashMap<
                                                    String,
                                                    String,
                                                > = std::collections::HashMap::new();
                                                for f in &all_formulas {
                                                    for dep in &f.dependencies.runtime {
                                                        if let Some(ref req) = dep.version_req {
                                                            constraint_map
                                                                .entry(dep.name.clone())
                                                                .or_insert_with(|| req.to_string());
                                                        }
                                                    }
                                                }

                                                println!(
                                                    "Resolved {} package(s) for {}:\n",
                                                    resolved_map.len(),
                                                    package
                                                );
                                                for entry in &summary.to_install {
                                                    let tag = if entry.is_dependency {
                                                        " (dependency)"
                                                    } else {
                                                        ""
                                                    };
                                                    if resolved_map.contains_key(&entry.name) {
                                                        let satisfies = constraint_map
                                                            .get(&entry.name)
                                                            .map(|c| format!(" (satisfies {})", c))
                                                            .unwrap_or_default();
                                                        println!(
                                                            "  {} {}{}{}",
                                                            entry.name,
                                                            entry.version,
                                                            satisfies,
                                                            tag
                                                        );
                                                    } else {
                                                        println!("  {} {}{}", entry.name, entry.version, tag);
                                                    }
                                                }
                                                if !summary.already_installed.is_empty() {
                                                    println!();
                                                    print!("{}", summary);
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Error resolving dependencies: {}", e);
                                                std::process::exit(1);
                                            }
                                        }
                                    } else {
                                        match queue.resolve() {
                                            Ok(items) => {
                                                if items.is_empty() {
                                                    println!("{} is already installed", package);
                                                } else {
                                                    println!("Would install {} package(s):", items.len());
                                                    for item in items {
                                                        println!(
                                                            "  {} {}",
                                                            item.formula.name(),
                                                            item.formula.version()
                                                        );
                                                    }
                                                    println!("\n(Actual installation not yet implemented)");
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Error resolving dependencies: {}", e);
                                                std::process::exit(1);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Formula not found: {}", e);
                                    eprintln!("\nTo search for packages:");
                                    eprintln!("  brew-rs search {}", package);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error initializing tap manager: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Uninstall { package } => {
            info!("Uninstalling package: {}", package);
            println!("🗑️  Uninstalling {} (not yet implemented)", package);
        }
        Commands::Search { query } => {
            info!("Searching for: {}", query);
            match brew_config::Config::load() {
                Ok(config) => {
                    match brew_tap::TapManager::new(config.paths) {
                        Ok(manager) => {
                            match manager.search_with_details(&query) {
                                Ok(results) => {
                                    if results.is_empty() {
                                        println!("No formulas found matching '{}'", query);
                                    } else {
                                        println!("Found {} formula(s) matching '{}':", results.len(), query);
                                        for entry in results {
                                            println!("  {} {} ({})", entry.name, entry.version, entry.tap_name);
                                            if !entry.description.is_empty() {
                                                println!("    {}", entry.description);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error searching: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error initializing tap manager: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Info { package } => {
            info!("Getting info for: {}", package);
            println!("ℹ️  Info for {} (not yet implemented)", package);
        }
        Commands::List => {
            info!("Listing installed packages");
            match brew_config::Config::load() {
                Ok(config) => {
                    match brew_core::Database::open(&config.paths) {
                        Ok(db) => {
                            match db.packages().list_all() {
                                Ok(packages) => {
                                    if packages.is_empty() {
                                        println!("No packages installed.");
                                        println!("\nTo install a package:");
                                        println!("  brew-rs install <package>");
                                    } else {
                                        println!("Installed packages ({}):", packages.len());
                                        for pkg in packages {
                                            let mut flags = Vec::new();
                                            if pkg.linked { flags.push("linked"); }
                                            if pkg.pinned { flags.push("pinned"); }

                                            let tap_info = pkg.tap.as_deref().unwrap_or("local");
                                            if flags.is_empty() {
                                                println!("  {} {} ({})", pkg.name, pkg.version, tap_info);
                                            } else {
                                                println!("  {} {} ({}) [{}]", pkg.name, pkg.version, tap_info, flags.join(", "));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error listing packages: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error opening database: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading configuration: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Update => {
            info!("Updating repositories");
            println!("🔄 Updating repositories (not yet implemented)");
        }
        Commands::Upgrade { package } => {
            if let Some(pkg) = package {
                info!("Upgrading package: {}", pkg);
                println!("⬆️  Upgrading {} (not yet implemented)", pkg);
            } else {
                info!("Upgrading all packages");
                println!("⬆️  Upgrading all packages (not yet implemented)");
            }
        }
        Commands::Tap { command } => {
            match command {
                TapCommands::Add { name, url } => {
                    info!("Adding tap: {} from {}", name, url);
                    match brew_config::Config::load() {
                        Ok(config) => {
                            match brew_tap::TapManager::new(config.paths) {
                                Ok(mut manager) => {
                                    match manager.add_tap(&name, &url) {
                                        Ok(_) => {
                                            println!("Added tap: {}", name);
                                            println!("  URL: {}", url);
                                        }
                                        Err(e) => {
                                            eprintln!("Error adding tap: {}", e);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error initializing tap manager: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error loading configuration: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                TapCommands::Remove { name } => {
                    info!("Removing tap: {}", name);
                    match brew_config::Config::load() {
                        Ok(config) => {
                            match brew_tap::TapManager::new(config.paths) {
                                Ok(mut manager) => {
                                    match manager.remove_tap(&name) {
                                        Ok(_) => {
                                            println!("Removed tap: {}", name);
                                        }
                                        Err(e) => {
                                            eprintln!("Error removing tap: {}", e);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error initializing tap manager: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error loading configuration: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                TapCommands::Update { name } => {
                    match brew_config::Config::load() {
                        Ok(config) => {
                            match brew_tap::TapManager::new(config.paths) {
                                Ok(mut manager) => {
                                    if let Some(tap_name) = name {
                                        info!("Updating tap: {}", tap_name);
                                        match manager.update_tap(&tap_name) {
                                            Ok(_) => println!("Updated tap: {}", tap_name),
                                            Err(e) => {
                                                eprintln!("Error updating tap {}: {}", tap_name, e);
                                                std::process::exit(1);
                                            }
                                        }
                                    } else {
                                        info!("Updating all taps");
                                        match manager.update_all() {
                                            Ok(_) => println!("Updated all taps"),
                                            Err(e) => {
                                                eprintln!("Error updating taps: {}", e);
                                                std::process::exit(1);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error initializing tap manager: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error loading configuration: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                TapCommands::List => {
                    match brew_config::Config::load() {
                        Ok(config) => {
                            match brew_tap::TapManager::new(config.paths) {
                                Ok(manager) => {
                                    let taps = manager.list_taps();
                                    if taps.is_empty() {
                                        println!("No taps installed.");
                                        println!("\nTo add a tap:");
                                        println!("  brew-rs tap add <name> <url>");
                                    } else {
                                        println!("Installed taps ({}):", taps.len());
                                        for tap in taps {
                                            println!("  {} ({})", tap.name, tap.url);
                                            if let Some(updated) = &tap.last_updated {
                                                println!("    Last updated: {}", updated.format("%Y-%m-%d %H:%M:%S"));
                                            }
                                            if let Some(commit) = &tap.commit_hash {
                                                let short_commit = if commit.len() > 8 {
                                                    &commit[..8]
                                                } else {
                                                    commit
                                                };
                                                println!("    Commit: {}", short_commit);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error initializing tap manager: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error loading configuration: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
