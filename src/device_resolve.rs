// Shared device-selection parsing and resolution used by both `capture` and
// `play`. The `--device` argument becomes `DeviceSelector::Index(n)` when
// numeric and `DeviceSelector::Name(s)` otherwise (the library matches names
// as case-insensitive substrings). The `--device-id` argument becomes
// `DeviceSelector::Id(s)`, matched by exact equality against the stable
// per-host device IDs reported by `decibri devices --json`. Input and output
// devices are separate namespaces, so the per-kind resolvers each hit their
// own enumeration function.

use anyhow::{Context, Result};
use decibri::{input_devices, output_devices, DeviceSelector};

use crate::exit;

pub fn parse_device_selector(s: &str) -> DeviceSelector {
    if let Ok(idx) = s.parse::<usize>() {
        DeviceSelector::Index(idx)
    } else {
        DeviceSelector::Name(s.to_string())
    }
}

/// Build the selector from the mutually exclusive `--device` / `--device-id`
/// flags (clap rejects supplying both). Neither flag means the system
/// default device.
fn selector_from_flags(device: Option<&str>, device_id: Option<&str>) -> DeviceSelector {
    match (device_id, device) {
        (Some(id), _) => DeviceSelector::Id(id.to_string()),
        (None, Some(arg)) => parse_device_selector(arg),
        (None, None) => DeviceSelector::Default,
    }
}

/// Resolve the input-device flags to a validated `DeviceSelector`.
///
/// With neither flag set this returns `DeviceSelector::Default` without
/// touching the audio subsystem. Otherwise the input device list is
/// enumerated up front so a selector that matches nothing produces a helpful
/// error (exit 3) before capture starts.
pub fn resolve_input(device: Option<&str>, device_id: Option<&str>) -> Result<DeviceSelector> {
    if device.is_none() && device_id.is_none() {
        return Ok(DeviceSelector::Default);
    }
    let devices = input_devices()
        .context("failed to enumerate input devices")
        .map_err(|e| exit::io(format!("{e:#}")))?;
    let selector = selector_from_flags(device, device_id);
    let (matched, kind, raw) = match &selector {
        DeviceSelector::Index(idx) => (
            devices.iter().any(|d| d.index == *idx),
            "index",
            device.unwrap_or_default(),
        ),
        DeviceSelector::Name(name) => {
            let lower = name.to_lowercase();
            (
                devices
                    .iter()
                    .any(|d| d.name.to_lowercase().contains(&lower)),
                "name",
                device.unwrap_or_default(),
            )
        }
        // An empty ID never matches: hosts report an empty string for
        // devices with no assignable ID, and the library skips those during
        // its own ID resolution.
        DeviceSelector::Id(id) => (
            !id.is_empty() && devices.iter().any(|d| d.id == *id),
            "id",
            device_id.unwrap_or_default(),
        ),
        // `DeviceSelector` is non-exhaustive. Selectors this resolver does
        // not build (`Default` and any future variant) pass through for the
        // library to resolve.
        _ => (true, "", ""),
    };
    if !matched {
        let list = devices
            .iter()
            .map(|d| format!("  {}: {}", d.index, d.name))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(exit::device_not_found(format!(
            "no input device matches {kind} \"{raw}\"\nAvailable input devices:\n{list}"
        )));
    }
    Ok(selector)
}

/// Resolve the output-device flags to a validated `DeviceSelector`.
///
/// Same contract as [`resolve_input`], against the output device list.
pub fn resolve_output(device: Option<&str>, device_id: Option<&str>) -> Result<DeviceSelector> {
    if device.is_none() && device_id.is_none() {
        return Ok(DeviceSelector::Default);
    }
    let devices = output_devices()
        .context("failed to enumerate output devices")
        .map_err(|e| exit::io(format!("{e:#}")))?;
    let selector = selector_from_flags(device, device_id);
    let (matched, kind, raw) = match &selector {
        DeviceSelector::Index(idx) => (
            devices.iter().any(|d| d.index == *idx),
            "index",
            device.unwrap_or_default(),
        ),
        DeviceSelector::Name(name) => {
            let lower = name.to_lowercase();
            (
                devices
                    .iter()
                    .any(|d| d.name.to_lowercase().contains(&lower)),
                "name",
                device.unwrap_or_default(),
            )
        }
        // An empty ID never matches: hosts report an empty string for
        // devices with no assignable ID, and the library skips those during
        // its own ID resolution.
        DeviceSelector::Id(id) => (
            !id.is_empty() && devices.iter().any(|d| d.id == *id),
            "id",
            device_id.unwrap_or_default(),
        ),
        // `DeviceSelector` is non-exhaustive. Selectors this resolver does
        // not build (`Default` and any future variant) pass through for the
        // library to resolve.
        _ => (true, "", ""),
    };
    if !matched {
        let list = devices
            .iter()
            .map(|d| format!("  {}: {}", d.index, d.name))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(exit::device_not_found(format!(
            "no output device matches {kind} \"{raw}\"\nAvailable output devices:\n{list}"
        )));
    }
    Ok(selector)
}

pub fn input_display_name(device: Option<&str>, device_id: Option<&str>) -> String {
    match input_devices().ok() {
        Some(list) => match selector_from_flags(device, device_id) {
            DeviceSelector::Index(idx) => list
                .iter()
                .find(|d| d.index == idx)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| device.unwrap_or_default().to_string()),
            DeviceSelector::Name(name) => {
                let lower = name.to_lowercase();
                list.iter()
                    .find(|d| d.name.to_lowercase().contains(&lower))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device.unwrap_or_default().to_string())
            }
            DeviceSelector::Id(id) => list
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| device_id.unwrap_or_default().to_string()),
            // `Default` and any future variant: show the system default
            // device's name when it can be determined.
            _ => list
                .into_iter()
                .find(|d| d.is_default)
                .map(|d| d.name)
                .unwrap_or_else(|| "default".to_string()),
        },
        None => device_id
            .or(device)
            .map(str::to_string)
            .unwrap_or_else(|| "default".to_string()),
    }
}

pub fn output_display_name(device: Option<&str>, device_id: Option<&str>) -> String {
    match output_devices().ok() {
        Some(list) => match selector_from_flags(device, device_id) {
            DeviceSelector::Index(idx) => list
                .iter()
                .find(|d| d.index == idx)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| device.unwrap_or_default().to_string()),
            DeviceSelector::Name(name) => {
                let lower = name.to_lowercase();
                list.iter()
                    .find(|d| d.name.to_lowercase().contains(&lower))
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device.unwrap_or_default().to_string())
            }
            DeviceSelector::Id(id) => list
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| device_id.unwrap_or_default().to_string()),
            // `Default` and any future variant: show the system default
            // device's name when it can be determined.
            _ => list
                .into_iter()
                .find(|d| d.is_default)
                .map(|d| d.name)
                .unwrap_or_else(|| "default".to_string()),
        },
        None => device_id
            .or(device)
            .map(str::to_string)
            .unwrap_or_else(|| "default".to_string()),
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

    #[test]
    fn selector_from_flags_precedence() {
        match selector_from_flags(None, Some("abc-id")) {
            DeviceSelector::Id(s) if s == "abc-id" => {}
            other => panic!("expected Id(\"abc-id\"), got {other:?}"),
        }
        match selector_from_flags(Some("3"), None) {
            DeviceSelector::Index(3) => {}
            other => panic!("expected Index(3), got {other:?}"),
        }
        match selector_from_flags(None, None) {
            DeviceSelector::Default => {}
            other => panic!("expected Default, got {other:?}"),
        }
    }
}
