// Shared --device argument parsing and resolution used by both `capture`
// and `play`. Numeric arguments become `DeviceSelector::Index(n)`; everything
// else becomes `DeviceSelector::Name(s)` for the library's case-insensitive
// substring match. Input and output devices are separate namespaces, so the
// per-kind resolvers each hit their own enumerate function.

use anyhow::{Context, Result};
use decibri::device::{enumerate_input_devices, enumerate_output_devices, DeviceSelector};

use crate::exit;

pub fn parse_device_selector(s: &str) -> DeviceSelector {
    if let Ok(idx) = s.parse::<usize>() {
        DeviceSelector::Index(idx)
    } else {
        DeviceSelector::Name(s.to_string())
    }
}

pub fn resolve_input_selector(arg: &str) -> Result<DeviceSelector> {
    let devices = enumerate_input_devices()
        .context("failed to enumerate input devices")
        .map_err(|e| exit::io(format!("{e:#}")))?;
    let selector = parse_device_selector(arg);
    let matched = match &selector {
        DeviceSelector::Index(idx) => devices.iter().any(|d| d.index == *idx),
        DeviceSelector::Name(name) => {
            let lower = name.to_lowercase();
            devices
                .iter()
                .any(|d| d.name.to_lowercase().contains(&lower))
        }
        DeviceSelector::Default => true,
    };
    if !matched {
        let list = devices
            .iter()
            .map(|d| format!("  {}: {}", d.index, d.name))
            .collect::<Vec<_>>()
            .join("\n");
        let kind = if matches!(selector, DeviceSelector::Index(_)) {
            "index"
        } else {
            "name"
        };
        return Err(exit::device_not_found(format!(
            "no input device matches {kind} \"{arg}\"\nAvailable input devices:\n{list}"
        )));
    }
    Ok(selector)
}

pub fn resolve_output_selector(arg: &str) -> Result<DeviceSelector> {
    let devices = enumerate_output_devices()
        .context("failed to enumerate output devices")
        .map_err(|e| exit::io(format!("{e:#}")))?;
    let selector = parse_device_selector(arg);
    let matched = match &selector {
        DeviceSelector::Index(idx) => devices.iter().any(|d| d.index == *idx),
        DeviceSelector::Name(name) => {
            let lower = name.to_lowercase();
            devices
                .iter()
                .any(|d| d.name.to_lowercase().contains(&lower))
        }
        DeviceSelector::Default => true,
    };
    if !matched {
        let list = devices
            .iter()
            .map(|d| format!("  {}: {}", d.index, d.name))
            .collect::<Vec<_>>()
            .join("\n");
        let kind = if matches!(selector, DeviceSelector::Index(_)) {
            "index"
        } else {
            "name"
        };
        return Err(exit::device_not_found(format!(
            "no output device matches {kind} \"{arg}\"\nAvailable output devices:\n{list}"
        )));
    }
    Ok(selector)
}

pub fn input_display_name(arg: Option<&str>) -> String {
    let devices = enumerate_input_devices().ok();
    match (arg, devices) {
        (Some(arg), Some(list)) => match parse_device_selector(arg) {
            DeviceSelector::Index(idx) => list
                .iter()
                .find(|d| d.index == idx)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| arg.to_string()),
            DeviceSelector::Name(name) => {
                let lower = name.to_lowercase();
                list.iter()
                    .find(|d| d.name.to_lowercase().contains(&lower))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| arg.to_string())
            }
            DeviceSelector::Default => arg.to_string(),
        },
        (Some(arg), None) => arg.to_string(),
        (None, Some(list)) => list
            .into_iter()
            .find(|d| d.is_default)
            .map(|d| d.name)
            .unwrap_or_else(|| "default".to_string()),
        (None, None) => "default".to_string(),
    }
}

pub fn output_display_name(arg: Option<&str>) -> String {
    let devices = enumerate_output_devices().ok();
    match (arg, devices) {
        (Some(arg), Some(list)) => match parse_device_selector(arg) {
            DeviceSelector::Index(idx) => list
                .iter()
                .find(|d| d.index == idx)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| arg.to_string()),
            DeviceSelector::Name(name) => {
                let lower = name.to_lowercase();
                list.iter()
                    .find(|d| d.name.to_lowercase().contains(&lower))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| arg.to_string())
            }
            DeviceSelector::Default => arg.to_string(),
        },
        (Some(arg), None) => arg.to_string(),
        (None, Some(list)) => list
            .into_iter()
            .find(|d| d.is_default)
            .map(|d| d.name)
            .unwrap_or_else(|| "default".to_string()),
        (None, None) => "default".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numeric_index() {
        match parse_device_selector("0") {
            DeviceSelector::Index(0) => {}
            other => panic!("expected Index(0), got {other:?}"),
        }
        match parse_device_selector("42") {
            DeviceSelector::Index(42) => {}
            other => panic!("expected Index(42), got {other:?}"),
        }
    }

    #[test]
    fn parse_name_substring() {
        match parse_device_selector("yeti") {
            DeviceSelector::Name(s) if s == "yeti" => {}
            other => panic!("expected Name(\"yeti\"), got {other:?}"),
        }
        match parse_device_selector("usb1") {
            DeviceSelector::Name(s) if s == "usb1" => {}
            other => panic!("expected Name(\"usb1\"), got {other:?}"),
        }
    }

    #[test]
    fn parse_negative_falls_to_name() {
        match parse_device_selector("-1") {
            DeviceSelector::Name(s) if s == "-1" => {}
            other => panic!("expected Name(\"-1\"), got {other:?}"),
        }
    }
}
