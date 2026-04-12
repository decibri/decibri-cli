use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Args;
use console::Term;
use crossbeam_channel::RecvTimeoutError;
use decibri::capture::{AudioCapture, AudioChunk, CaptureConfig};
use decibri::device::{enumerate_input_devices, DeviceSelector};
use hound::{SampleFormat, WavSpec, WavWriter};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::exit;

// Watchdog high-water mark for the library's *unbounded* internal channel.
//
// decibri 3.0.0's `AudioCapture::start()` hard-codes `crossbeam_channel::unbounded()`
// and exposes only the `Receiver`. The CLI cannot inject `try_send` backpressure
// from the producer side, so we approximate bounded behaviour from the consumer
// side: every chunk we drain, we check `receiver.len()`; if it exceeds this
// high-water mark, the writer has fallen too far behind, we stop the stream and
// surface an IO error with the partial recording preserved.
//
// 256 chunks at the default `frames_per_buffer = 1600` @ 16 kHz mono is roughly
// 16 seconds of buffered audio — generous headroom for transient OS flushes
// without letting a genuinely stuck writer balloon memory. Revisit when decibri
// upstreams a `CaptureConfig::channel_capacity` option (tracked in BUILD-PLAN.md
// "Known issues for v0.2.0").
const WATCHDOG_HIGH_WATER: usize = 256;

/// How long we drain the receiver after `stop()` to catch any chunks the
/// library flushed but we hadn't yet read. Without this, the last ~100ms of
/// audio can be lost on Ctrl+C.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// Loop poll interval. Keeps the drain loop responsive to ctrlc, duration
/// expiry, and stream-health checks without busy-waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Frames per cpal callback buffer. The library default; exposed here as a
/// constant so the watchdog comment stays in sync if it ever changes.
const FRAMES_PER_BUFFER: u32 = 1600;

#[derive(Args)]
pub struct CaptureArgs {
    /// Output WAV file path.
    #[arg(long, short = 'o')]
    pub output: PathBuf,

    /// Recording duration. Accepts bare seconds (10, 5.5) or humantime
    /// strings (10s, 1m30s). If omitted, records until Ctrl+C.
    #[arg(long, short = 'd', value_parser = parse_duration)]
    pub duration: Option<Duration>,

    /// Sample rate in Hz. Default 16000 (voice). Music preset: 44100.
    #[arg(long, short = 'r', default_value_t = 16000)]
    pub rate: u32,

    /// Number of channels. 1 = mono (default), 2 = stereo.
    #[arg(long, short = 'c', default_value_t = 1)]
    pub channels: u16,

    /// Device name substring (case-insensitive) or numeric index from `decibri devices`.
    #[arg(long)]
    pub device: Option<String>,
}

/// Parse the --device argument into a DeviceSelector. Numeric values become
/// `Index(n)`; everything else becomes `Name(s)` for substring matching.
pub(crate) fn parse_device_selector(s: &str) -> DeviceSelector {
    if let Ok(idx) = s.parse::<usize>() {
        DeviceSelector::Index(idx)
    } else {
        DeviceSelector::Name(s.to_string())
    }
}

pub(crate) fn parse_duration(s: &str) -> std::result::Result<Duration, String> {
    if let Ok(secs) = s.parse::<f64>() {
        if !secs.is_finite() || secs < 0.0 {
            return Err("duration must be finite and non-negative".into());
        }
        return Ok(Duration::from_secs_f64(secs));
    }
    humantime::parse_duration(s).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct CaptureCompletion {
    file: String,
    duration_seconds: f64,
    sample_rate: u32,
    channels: u16,
    samples: u64,
    bytes: u64,
    device: String,
    dropped_chunks: u64,
}

#[derive(Debug)]
enum ExitReason {
    Normal,
    Watchdog,
    DeviceLost,
}

pub fn run(args: CaptureArgs, json: bool, quiet: bool) -> Result<()> {
    // Pre-validate the device argument against the input device list so we can
    // give a helpful error before touching the audio subsystem (and exit 3, not 4).
    let selector = match &args.device {
        Some(s) => Some(resolve_device_arg(s)?),
        None => None,
    };

    let device_name = resolve_device_name(args.device.as_deref());

    let config = CaptureConfig {
        sample_rate: args.rate,
        channels: args.channels,
        frames_per_buffer: FRAMES_PER_BUFFER,
        device: selector.unwrap_or(DeviceSelector::Default),
    };

    let capture = AudioCapture::new(config).map_err(|e| anyhow!("capture init failed: {e}"))?;

    let spec = WavSpec {
        channels: args.channels,
        sample_rate: args.rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&args.output, spec)
        .with_context(|| format!("failed to create {}", args.output.display()))
        .map_err(|e| exit::io(format!("{e:#}")))?;

    // Install ctrlc handler. Idempotent across runs but `set_handler` errors
    // if called twice in the same process — fine to ignore in tests/REPLs.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_handler = shutdown.clone();
    let _ = ctrlc::set_handler(move || {
        shutdown_handler.store(true, Ordering::SeqCst);
    });

    let stream = capture
        .start()
        .map_err(|e| exit::io(format!("capture start failed: {e}")))?;

    if !json && !quiet {
        eprintln!(
            "Recording to {} ({} Hz, {} channel{}, 16-bit PCM WAV)",
            args.output.display(),
            args.rate,
            args.channels,
            if args.channels == 1 { "" } else { "s" }
        );
        eprintln!("Device: {device_name}");
    }

    let show_progress = !quiet && !json && Term::stderr().features().is_attended();
    let progress = if show_progress {
        Some(make_progress_bar(args.duration))
    } else {
        None
    };

    let started = Instant::now();
    let mut samples_written: u64 = 0;
    let mut exit_reason = ExitReason::Normal;
    let receiver = stream.receiver().clone();

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        if let Some(d) = args.duration {
            if started.elapsed() >= d {
                break;
            }
        }
        if !stream.is_open() && receiver.is_empty() {
            exit_reason = ExitReason::DeviceLost;
            break;
        }

        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(chunk) => {
                samples_written +=
                    write_chunk(&mut writer, &chunk).map_err(|e| exit::io(format!("{e:#}")))?;
                if let Some(pb) = &progress {
                    update_progress(pb, started.elapsed(), args.duration, samples_written);
                }
                if receiver.len() > WATCHDOG_HIGH_WATER {
                    exit_reason = ExitReason::Watchdog;
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                exit_reason = ExitReason::DeviceLost;
                break;
            }
        }
    }

    // Cooperative shutdown: stop the producer flag, drain remaining buffered
    // chunks for up to DRAIN_TIMEOUT (skipped on watchdog trip — point of the
    // trip is "writer can't keep up", so adding more chunks is wrong).
    stream.stop();
    if matches!(exit_reason, ExitReason::Normal) {
        let drain_start = Instant::now();
        while drain_start.elapsed() < DRAIN_TIMEOUT {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(chunk) => {
                    samples_written +=
                        write_chunk(&mut writer, &chunk).map_err(|e| exit::io(format!("{e:#}")))?;
                }
                Err(_) => break,
            }
        }
    }
    drop(stream); // cpal Stream stops on Drop.

    let elapsed = started.elapsed();
    let finalize_result = writer
        .finalize()
        .context("failed to finalize WAV")
        .map_err(|e| exit::io(format!("{e:#}")));

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    finalize_result?;

    match exit_reason {
        ExitReason::DeviceLost => {
            return Err(exit::io(format!(
                "audio device became unavailable during capture. \
                 Partial recording preserved at {} ({:.1}s captured)",
                args.output.display(),
                elapsed.as_secs_f64()
            )));
        }
        ExitReason::Watchdog => {
            return Err(exit::io(format!(
                "writer could not keep up with capture (>{} chunks buffered). \
                 Partial recording preserved at {} ({:.1}s captured)",
                WATCHDOG_HIGH_WATER,
                args.output.display(),
                elapsed.as_secs_f64()
            )));
        }
        ExitReason::Normal => {}
    }

    let bytes = samples_written * 2; // i16 = 2 bytes per sample
    let duration_seconds = if args.rate > 0 && args.channels > 0 {
        samples_written as f64 / (args.rate as f64 * args.channels as f64)
    } else {
        0.0
    };

    if json {
        let payload = CaptureCompletion {
            file: args.output.display().to_string(),
            duration_seconds,
            sample_rate: args.rate,
            channels: args.channels,
            samples: samples_written,
            bytes,
            device: device_name,
            dropped_chunks: 0,
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !quiet {
        eprintln!(
            "Done. {:.1}s captured ({} samples, {} KB).",
            duration_seconds,
            samples_written,
            bytes / 1024
        );
    }

    Ok(())
}

/// Convert one f32 chunk to i16 and stream it into the WAV writer.
/// Returns the number of i16 samples written.
fn write_chunk<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    chunk: &AudioChunk,
) -> Result<u64> {
    for &sample in &chunk.data {
        writer.write_sample(f32_to_i16(sample))?;
    }
    Ok(chunk.data.len() as u64)
}

/// Standard lossy f32 → i16 PCM conversion. Clamp protects against the rare
/// out-of-range sample (some audio backends overshoot slightly under load).
pub(crate) fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

fn make_progress_bar(duration: Option<Duration>) -> ProgressBar {
    let pb = match duration {
        Some(d) => {
            let total_ms = d.as_millis().max(1) as u64;
            let pb = ProgressBar::new(total_ms);
            pb.set_style(
                ProgressStyle::with_template(
                    "Recording: [{elapsed_precise}] [{bar:30.cyan/blue}] {msg}",
                )
                .expect("indicatif template")
                .progress_chars("=> "),
            );
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("Recording: [{elapsed_precise}] {spinner} {msg}")
                    .expect("indicatif template"),
            );
            pb
        }
    };
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

fn update_progress(pb: &ProgressBar, elapsed: Duration, duration: Option<Duration>, samples: u64) {
    let kb = (samples * 2) / 1024;
    pb.set_message(format!("{samples} samples | {kb} KB"));
    if duration.is_some() {
        pb.set_position(elapsed.as_millis() as u64);
    }
}

/// Pre-flight validation: parse the user's --device argument, look it up in
/// the input device list, and return the resolved `DeviceSelector`. On no
/// match, returns a `DeviceNotFound` error (exit code 3) listing all
/// available input devices.
fn resolve_device_arg(arg: &str) -> Result<DeviceSelector> {
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

fn resolve_device_name(user: Option<&str>) -> String {
    let devices = enumerate_input_devices().ok();
    match (user, devices) {
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
    fn parse_device_numeric_index() {
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
    fn parse_device_name_substring() {
        match parse_device_selector("yeti") {
            DeviceSelector::Name(s) if s == "yeti" => {}
            other => panic!("expected Name(\"yeti\"), got {other:?}"),
        }
        // Mixed alphanumeric falls through to Name (parse::<usize> rejects it).
        match parse_device_selector("usb1") {
            DeviceSelector::Name(s) if s == "usb1" => {}
            other => panic!("expected Name(\"usb1\"), got {other:?}"),
        }
    }

    #[test]
    fn parse_device_negative_falls_to_name() {
        // "-1" doesn't parse as usize, so it becomes a name (and would fail
        // to match any real device — exit 3 with the not-found path).
        match parse_device_selector("-1") {
            DeviceSelector::Name(s) if s == "-1" => {}
            other => panic!("expected Name(\"-1\"), got {other:?}"),
        }
    }
}
