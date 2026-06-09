//! Parcel CLI — the `parcel` command-line tool.
//!
//! Subcommands:
//! - `init`    — scaffold a `parcel.json` and `parcel/` resource directory.
//! - `build`   — compile the installer from the current configuration.
//! - `preview` — launch a live preview of the installer UI.

mod build;
mod init;
mod preview;
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init::run(),
        Command::Build => build::run(),
        Command::Preview => preview::run(),
    }
}
