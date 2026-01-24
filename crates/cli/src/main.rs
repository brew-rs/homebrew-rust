use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber;

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
        Commands::Install { package } => {
            info!("Installing package: {}", package);
            // TODO: Implement installation logic
            println!("📦 Installing {} (not yet implemented)", package);
        }
        Commands::Uninstall { package } => {
            info!("Uninstalling package: {}", package);
            println!("🗑️  Uninstalling {} (not yet implemented)", package);
        }
        Commands::Search { query } => {
            info!("Searching for: {}", query);
            println!("🔍 Searching for '{}' (not yet implemented)", query);
        }
        Commands::Info { package } => {
            info!("Getting info for: {}", package);
            println!("ℹ️  Info for {} (not yet implemented)", package);
        }
        Commands::List => {
            info!("Listing installed packages");
            println!("📋 Installed packages (not yet implemented)");
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
                            let mut manager = brew_tap::TapManager::new(config.paths);
                            match manager.add_tap(&name, &url) {
                                Ok(_) => {
                                    println!("✓ Added tap: {}", name);
                                    println!("  URL: {}", url);
                                }
                                Err(e) => {
                                    eprintln!("Error adding tap: {}", e);
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
                            let _manager = brew_tap::TapManager::new(config.paths);
                            // For now, just show a message - we need to persist taps
                            if let Some(tap_name) = name {
                                info!("Updating tap: {}", tap_name);
                                println!("🔄 Updating tap {} (tap persistence not yet implemented)", tap_name);
                            } else {
                                info!("Updating all taps");
                                println!("🔄 Updating all taps (tap persistence not yet implemented)");
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
                            let _manager = brew_tap::TapManager::new(config.paths);
                            println!("📋 Installed taps (tap persistence not yet implemented)");
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
