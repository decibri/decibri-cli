use anyhow::{Context, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table};
use decibri::device::{
    enumerate_input_devices, enumerate_output_devices, DeviceInfo, OutputDeviceInfo,
};
use serde::Serialize;

#[derive(Args)]
pub struct DevicesArgs {
    /// List input devices only.
    #[arg(long, conflicts_with = "output")]
    pub input: bool,

    /// List output devices only.
    #[arg(long)]
    pub output: bool,
}

#[derive(Serialize)]
struct DeviceJson {
    index: usize,
    name: String,
    kind: &'static str,
    default: bool,
    channels: u16,
    sample_rate: u32,
}

#[derive(Serialize, Default)]
struct DevicesOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    input_devices: Option<Vec<DeviceJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_devices: Option<Vec<DeviceJson>>,
}

impl From<&DeviceInfo> for DeviceJson {
    fn from(d: &DeviceInfo) -> Self {
        Self {
            index: d.index,
            name: d.name.clone(),
            kind: "input",
            default: d.is_default,
            channels: d.max_input_channels,
            sample_rate: d.default_sample_rate,
        }
    }
}

impl From<&OutputDeviceInfo> for DeviceJson {
    fn from(d: &OutputDeviceInfo) -> Self {
        Self {
            index: d.index,
            name: d.name.clone(),
            kind: "output",
            default: d.is_default,
            channels: d.max_output_channels,
            sample_rate: d.default_sample_rate,
        }
    }
}

pub fn run(args: DevicesArgs, json: bool, quiet: bool) -> Result<()> {
    let show_input = args.input || !args.output;
    let show_output = args.output || !args.input;

    let input = if show_input {
        Some(
            enumerate_input_devices()
                .context("failed to enumerate input devices")
                .map_err(io_error)?,
        )
    } else {
        None
    };

    let output = if show_output {
        Some(
            enumerate_output_devices()
                .context("failed to enumerate output devices")
                .map_err(io_error)?,
        )
    } else {
        None
    };

    if json {
        let payload = DevicesOutput {
            input_devices: input
                .as_ref()
                .map(|v| v.iter().map(DeviceJson::from).collect()),
            output_devices: output
                .as_ref()
                .map(|v| v.iter().map(DeviceJson::from).collect()),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if let Some(devices) = &input {
        if devices.is_empty() {
            if !quiet {
                println!("No input devices found.");
            }
        } else {
            if !quiet {
                println!("Input devices:");
            }
            print_table(devices.iter().map(|d| Row {
                index: d.index,
                name: &d.name,
                channels: d.max_input_channels,
                rate: d.default_sample_rate,
                default: d.is_default,
            }));
        }
    }

    if input.is_some() && output.is_some() && !quiet {
        println!();
    }

    if let Some(devices) = &output {
        if devices.is_empty() {
            if !quiet {
                println!("No output devices found.");
            }
        } else {
            if !quiet {
                println!("Output devices:");
            }
            print_table(devices.iter().map(|d| Row {
                index: d.index,
                name: &d.name,
                channels: d.max_output_channels,
                rate: d.default_sample_rate,
                default: d.is_default,
            }));
        }
    }

    Ok(())
}

struct Row<'a> {
    index: usize,
    name: &'a str,
    channels: u16,
    rate: u32,
    default: bool,
}

fn print_table<'a, I: IntoIterator<Item = Row<'a>>>(rows: I) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Index", "Name", "Channels", "Rate", "Default"]);
    for row in rows {
        table.add_row(vec![
            row.index.to_string(),
            row.name.to_string(),
            row.channels.to_string(),
            format!("{} Hz", row.rate),
            if row.default {
                "✓".to_string()
            } else {
                String::new()
            },
        ]);
    }
    println!("{table}");
}

fn io_error(e: anyhow::Error) -> anyhow::Error {
    // Exit code 4 ("IO error") is enforced at main.rs by mapping any error
    // tagged with this marker to ExitCode(4). For Phase 2 we surface the error;
    // exit-code routing lands wholesale in a later phase when the table is wired.
    e
}
