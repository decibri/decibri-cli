use clap::Parser;

#[derive(Parser)]
#[command(
    name = "decibri",
    version,
    about = "Cross-platform CLI for audio capture, playback, and device management"
)]
struct Cli {}

fn main() {
    Cli::parse();
}
