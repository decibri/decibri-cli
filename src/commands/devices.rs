use anyhow::{Context, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table};
use decibri::{input_devices, output_devices, MicrophoneInfo, SpeakerInfo};
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
    id: String,
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

impl From<&MicrophoneInfo> for DeviceJson {
    fn from(d: &MicrophoneInfo) -> Self {
        Self {
            index: d.index,
            name: d.name.clone(),
            id: d.id.clone(),
            kind: "input",
            default: d.is_default,
            channels: d.max_input_channels,
            sample_rate: d.default_sample_rate,
        }
    }
}

impl From<&SpeakerInfo> for DeviceJson {
    fn from(d: &SpeakerInfo) -> Self {
        Self {
            index: d.index,
            name: d.name.clone(),
            id: d.id.clone(),
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
        Some(input_devices().context("failed to enumerate input devices")?)
    } else {
        None
    };

    let output = if show_output {
        Some(output_devices().context("failed to enumerate output devices")?)
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
