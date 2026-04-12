use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

mod commands;
mod exit;

#[derive(Parser)]
#[command(
    name = "decibri",
    version,
    about = "Cross-platform CLI for audio capture, playback, and device management"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Emit machine-readable JSON output where supported.
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential human output.
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version and build information.
    Version,
    /// List available audio input and output devices.
    Devices(commands::devices::DevicesArgs),
    /// Record audio from an input device to a WAV file.
    Capture(commands::capture::CaptureArgs),
    /// Generate shell completion scripts (plumbing only in v0.1.0).
    #[command(hide = true)]
    Completions {
        /// Target shell.
        shell: Shell,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(exit::classify(&err))
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Version) => commands::version::run(cli.json),
        Some(Commands::Devices(args)) => commands::devices::run(args, cli.json, cli.quiet),
        Some(Commands::Capture(args)) => commands::capture::run(args, cli.json, cli.quiet),
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            Ok(())
        }
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}
