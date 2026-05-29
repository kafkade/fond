use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fond_store::FondPaths;

/// fond — a private, local-first personal cooking & recipe manager.
#[derive(Parser)]
#[command(name = "fond", version, about)]
struct Cli {
    /// Data directory (default: platform-specific)
    #[arg(long, env = "FOND_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise the fond data directory and default structure.
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = FondPaths::resolve(cli.data_dir);

    match cli.command {
        Commands::Init => cmd_init(&paths)?,
    }

    Ok(())
}

fn cmd_init(paths: &FondPaths) -> Result<()> {
    paths
        .ensure_dirs()
        .context("failed to create fond data directories")?;

    println!("Initialised fond at {}", paths.data_dir.display());
    println!("  recipes/  — your .cook recipe files");
    println!("  config/   — fond configuration");
    Ok(())
}
