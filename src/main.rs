use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "decibri",
    version,
    about = "Cross-platform CLI for audio capture, playback, and device management"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version and build information.
    Version,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Version) => commands::version::run(cli.json),
        None => {
            Cli::parse_from(["decibri", "--help"]);
            Ok(())
        }
    }
}
