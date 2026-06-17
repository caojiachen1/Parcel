//! Parcel CLI — the `parcel` command-line tool.
//!
//! Subcommands:
//! - `init`    — scaffold a `parcel.json` and `parcel/` resource directory.
//! - `build`   — compile the installer from the current configuration.
//! - `preview` — launch a live preview of the installer UI.
//! - `clean`   — rollback build artifacts for local development.

mod build;
mod clean;
mod init;
mod preview;
mod setup;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "parcel", version, about = "Parcel — universal Tauri installer generator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialise Parcel configuration in the current directory.
    Init,
    /// Build the installer executable.
    Build,
    /// Preview the installer UI without building.
    Preview,
    /// Launch the Parcel Setup GUI for the current directory.
    Setup,
    /// Clean build artifacts (rollback `.parcel-build/`).
    ///
    /// By default only the build cache is removed.  Use `--dist` to
    /// also remove `dist/`, or `--all` for a full reset including
    /// `parcel.json` and `parcel/`.
    Clean {
        /// Also remove `dist/` output directory.
        #[arg(long)]
        dist: bool,
        /// Remove everything including `parcel.json` and `parcel/`.
        #[arg(long)]
        all: bool,
        /// Show what would be removed without actually removing.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init::run(),
        Command::Build => build::run(),
        Command::Preview => preview::run(),
        Command::Setup => setup::run(),
        Command::Clean { dist, all, dry_run } => clean::run_with_options(dist, all, dry_run),
    }
}
