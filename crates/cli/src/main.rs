use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, Level};

/// Build the text output for `brew-rs info`.
///
/// Returns `None` when the package is not found in either taps or the database
/// (caller should print an error and exit 1).
/// Returns `Some(text)` with the formatted info block otherwise.
fn build_info_text(
    tap_formula: Option<&brew_formula::Formula>,
    installed: Option<&brew_core::InstalledPackage>,
) -> Option<String> {
    match (tap_formula, installed) {
        (None, None) => None,
        _ => {
            let mut lines: Vec<String> = Vec::new();

            // Formula metadata from tap
            if let Some(f) = tap_formula {
                lines.push(format!("{}: {}", f.name(), f.package.description));
                lines.push(format!("Version:  {}", f.version()));
                if let Some(hp) = &f.package.homepage {
                    lines.push(format!("Homepage: {}", hp));
                }
                if let Some(lic) = &f.package.license {
                    lines.push(format!("License:  {}", lic));
                }
                lines.push(format!("Source:   {}", f.source.url));
                if !f.dependencies.runtime.is_empty() {
                    let deps: Vec<String> = f
                        .dependencies
                        .runtime
                        .iter()
                        .map(|d| d.to_string())
                        .collect();
                    lines.push(format!("Deps:     {}", deps.join(", ")));
                }
            }

            // Install status
            if let Some(pkg) = installed {
                lines.push(format!(
                    "Installed: {} ({})",
                    pkg.version,
                    pkg.cellar_path.display()
                ));
                let linked_str = if pkg.linked { "linked" } else { "not linked" };
                lines.push(format!("Status:    {}", linked_str));
                let ts = chrono::DateTime::from_timestamp(pkg.installed_at, 0)
                    .map(|dt: chrono::DateTime<chrono::Utc>| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                lines.push(format!("Date:      {}", ts));
            } else {
                lines.push("Not installed".to_string());
            }

            Some(lines.join("\n"))
        }
    }
}

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
        /// Remove even if other packages depend on it
        #[arg(long)]
        force: bool,
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
                                                    // Run the real install pipeline
                                                    let fetcher = match brew_fetcher::Fetcher::new() {
                                                        Ok(f) => f,
                                                        Err(e) => {
                                                            eprintln!("Failed to create fetcher: {}", e);
                                                            std::process::exit(1);
                                                        }
                                                    };
                                                    let installer = brew_core::installer::Installer::new(
                                                        config.paths.clone(),
                                                        fetcher,
                                                    );
                                                    let db = match brew_core::Database::open(&config.paths) {
                                                        Ok(d) => d,
                                                        Err(e) => {
                                                            eprintln!("Failed to open database: {}", e);
                                                            std::process::exit(1);
                                                        }
                                                    };

                                                    let mut installed_count = 0;
                                                    for item in &items {
                                                        let name = item.formula.name();
                                                        let version = item.formula.version();

                                                        let cellar = config.paths.package_cellar(name, version);
                                                        if db.packages().is_installed(name).unwrap_or(false) && cellar.exists() {
                                                            println!("{} {} already installed, skipping", name, version);
                                                            continue;
                                                        }

                                                        println!("Installing {} {}...", name, version);

                                                        match installer.install_formula(&item.formula).await {
                                                            Ok(result) => {
                                                                if let Err(e) = installer.record_install(&db, &item.formula, &result) {
                                                                    eprintln!("Warning: failed to record {} in database: {}", name, e);
                                                                }
                                                                println!("  {} {} installed", name, version);
                                                                installed_count += 1;
                                                            }
                                                            Err(e) => {
                                                                let _ = installer.record_failure(&db, &item.formula, &e.to_string());
                                                                eprintln!("Error installing {} {}: {:#}", name, version, e);
                                                                std::process::exit(1);
                                                            }
                                                        }
                                                    }

                                                    if installed_count > 0 {
                                                        println!("\nInstalled {} package(s)", installed_count);
                                                        if !config.paths.is_bin_in_path() {
                                                            println!("Note: add {} to your PATH to use installed binaries", config.paths.bin_dir.display());
                                                        }
                                                    }
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
        Commands::Uninstall { package, force } => {
            info!("Uninstalling package: {}", package);
            match brew_config::Config::load() {
                Ok(config) => {
                    match brew_core::Database::open(&config.paths) {
                        Ok(db) => {
                            match brew_core::uninstaller::uninstall(&db, &config.paths, &package, force) {
                                Ok(version) => {
                                    println!("Uninstalled {} {}", package, version);
                                }
                                Err(e) => {
                                    let msg = e.to_string();
                                    eprintln!("Error: {}", msg);
                                    if msg.contains("required by") && !force {
                                        eprintln!("Use --force to uninstall anyway.");
                                    }
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
            match brew_config::Config::load() {
                Ok(config) => {
                    let tap_formula = match brew_tap::TapManager::new(config.paths.clone()) {
                        Ok(mgr) => mgr.find_formula(&package).ok(),
                        Err(e) => {
                            tracing::warn!("Failed to initialize tap manager: {}", e);
                            None
                        }
                    };

                    let installed = match brew_core::Database::open(&config.paths) {
                        Ok(db) => db.packages().find_by_name(&package).ok().flatten(),
                        Err(e) => {
                            tracing::warn!("Failed to open database: {}", e);
                            None
                        }
                    };

                    match build_info_text(tap_formula.as_ref(), installed.as_ref()) {
                        None => {
                            eprintln!("Error: {} not found in any tap or installed packages", package);
                            std::process::exit(1);
                        }
                        Some(text) => {
                            println!("{}", text);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading configuration: {}", e);
                    std::process::exit(1);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use brew_core::InstalledPackage;
    use std::path::PathBuf;

    fn make_test_formula(name: &str, version: &str) -> brew_formula::Formula {
        brew_formula::Formula::from_str(&format!(
            "[package]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"Test {name}\"\n\
             [source]\nurl = \"https://example.com/{name}-{version}.tar.gz\"\n\
             sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n"
        ))
        .expect("test formula should be valid")
    }

    fn make_test_installed(name: &str, version: &str) -> InstalledPackage {
        InstalledPackage::new(
            name.to_string(),
            version.to_string(),
            PathBuf::from(format!("/tmp/cellar/{}/{}", name, version)),
        )
    }

    #[test]
    fn test_info_neither_returns_none() {
        assert!(build_info_text(None, None).is_none());
    }

    #[test]
    fn test_info_formula_only_shows_metadata_and_not_installed() {
        let f = make_test_formula("jq", "1.8.1");
        let output = build_info_text(Some(&f), None).unwrap();
        assert!(output.contains("jq"), "should include package name");
        assert!(output.contains("1.8.1"), "should include version");
        assert!(
            output.contains("Not installed"),
            "should say Not installed when package is absent from DB"
        );
    }

    #[test]
    fn test_info_installed_only_shows_install_status() {
        let pkg = make_test_installed("jq", "1.8.1");
        let output = build_info_text(None, Some(&pkg)).unwrap();
        assert!(output.contains("1.8.1"), "should include installed version");
        assert!(
            !output.contains("Not installed"),
            "should NOT say Not installed when package is present in DB"
        );
        assert!(
            output.contains("Installed:"),
            "should include Installed label"
        );
    }

    #[test]
    fn test_info_both_shows_formula_and_install_status() {
        let f = make_test_formula("curl", "8.12.1");
        let pkg = make_test_installed("curl", "8.12.1");
        let output = build_info_text(Some(&f), Some(&pkg)).unwrap();
        assert!(output.contains("curl"), "should include package name");
        assert!(output.contains("8.12.1"), "should include version");
        assert!(
            !output.contains("Not installed"),
            "should NOT say Not installed when package is installed"
        );
        assert!(
            output.contains("Installed:"),
            "should include Installed label"
        );
        assert!(
            output.contains("Source:"),
            "should include source URL from tap formula"
        );
    }
}
