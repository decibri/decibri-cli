use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Args;
use console::Term;
use decibri::{AudioChunk, DecibriError, HighpassFilter, Microphone, MicrophoneConfig};
use hound::{SampleFormat, WavSpec, WavWriter};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::device_resolve::{input_display_name, resolve_input};
use crate::exit;

// Capture pipeline notes.
//
// `MicrophoneStream::next_chunk` delivers exactly the requested number of
// interleaved samples per chunk, at the requested sample rate, on every
// device: the library opens the device at its native rate, resamples in its
// capture chain, and re-blocks on the consumer side. The final chunk at
// stream close may be shorter, carrying the remaining tail, so no captured
// sample is lost.
//
// The library's internal capture channel is bounded. If this writer loop
// stalls long enough for the channel to fill, the library drops the newest
// buffers and counts them; `overrun_count()` reports the total. Capture
// completes anyway: the count is surfaced as `dropped_chunks` in the JSON
// completion payload and as a stderr warning when nonzero.

/// Frames requested from the device per callback buffer, and the block size
/// (times channels) requested from `next_chunk`. 1600 frames of mono 16 kHz
/// audio is one 100 ms block.
const FRAMES_PER_BUFFER: u32 = 1600;

/// Blocking timeout for each `next_chunk` call. Keeps the loop responsive to
/// Ctrl+C, duration expiry, and stream-health checks without busy-waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Total budget for draining buffered audio after `stop()`. The library
/// delivers the remaining full blocks, then the final short tail, then
/// reports the stream closed; this bound only guards against a wedged
/// stream.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Args)]
pub struct CaptureArgs {
    /// Output WAV file path.
    #[arg(long, short = 'o')]
    pub output: PathBuf,

    /// Recording duration. Accepts bare seconds (10, 5.5) or humantime
    /// strings (10s, 1m30s). If omitted, records until Ctrl+C.
    #[arg(long, short = 'd', value_parser = parse_duration)]
    pub duration: Option<Duration>,

    /// Sample rate in Hz. Default 16000 (voice). Output is delivered at this
    /// rate on every device.
    #[arg(long, short = 'r', default_value_t = 16000)]
    pub rate: u32,

    /// Number of channels. Capture is mono only; accepts 1.
    #[arg(long, short = 'c', default_value_t = 1, value_parser = parse_channels)]
    pub channels: u16,

    /// Device name substring (case-insensitive) or numeric index from `decibri devices`.
    #[arg(long)]
    pub device: Option<String>,

    /// Stable device ID from `decibri devices --json` (exact match).
    #[arg(long, conflicts_with = "device")]
    pub device_id: Option<String>,

    /// Remove a constant DC offset from the captured signal.
    #[arg(long)]
    pub dc_removal: bool,

    /// Apply a high-pass filter at the given cutoff in Hz (removes
    /// low-frequency rumble). Supported cutoffs: 80, 100.
    #[arg(long, value_name = "HZ", value_parser = parse_highpass)]
    pub highpass: Option<u32>,

    /// Automatic gain control to the given target level in dBFS.
    /// Range: -40 to -3 (for example -20).
    #[arg(long, value_name = "DBFS", allow_negative_numbers = true, value_parser = parse_agc)]
    pub agc: Option<i8>,

    /// Peak limiter ceiling in dBFS. Range: -3.0 to 0.0 (for example -1).
    #[arg(long, value_name = "DBFS", allow_negative_numbers = true, value_parser = parse_limiter)]
    pub limiter: Option<f32>,
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

pub(crate) fn parse_channels(s: &str) -> std::result::Result<u16, String> {
    match s.parse::<u16>() {
        Ok(1) => Ok(1),
        Ok(_) => Err("capture is mono only; --channels accepts 1".into()),
        Err(e) => Err(e.to_string()),
    }
}

// The library's high-pass is a closed set of named cutoffs
// (`HighpassFilter`), not a free frequency. The flag takes the cutoff in Hz,
// the same integer form the decibri Python and Node bindings use, and
// rejects values outside the set with the bindings' wording.
pub(crate) fn parse_highpass(s: &str) -> std::result::Result<u32, String> {
    match s.parse::<u32>() {
        Ok(80) => Ok(80),
        Ok(100) => Ok(100),
        Ok(other) => Err(format!("highpass must be one of: 80, 100; got {other}")),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn parse_agc(s: &str) -> std::result::Result<i8, String> {
    match s.parse::<i8>() {
        Ok(target) if (-40..=-3).contains(&target) => Ok(target),
        Ok(target) => Err(format!("agc must be in [-40, -3]; got {target}")),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn parse_limiter(s: &str) -> std::result::Result<f32, String> {
    match s.parse::<f32>() {
        Ok(ceiling) if (-3.0..=0.0).contains(&ceiling) => Ok(ceiling),
        Ok(ceiling) => Err(format!("limiter must be in [-3.0, 0.0]; got {ceiling}")),
        Err(e) => Err(e.to_string()),
    }
}

/// Map the validated cutoff in Hz to the library's closed-set selector.
fn highpass_filter(hz: u32) -> HighpassFilter {
    match hz {
        80 => HighpassFilter::Hz80,
        100 => HighpassFilter::Hz100,
        other => unreachable!("parse_highpass admits only 80 and 100, got {other}"),
    }
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
    conditioning: ConditioningReport,
}

/// The conditioning stages active for this capture, one key per active
/// stage. Always present in the completion payload, `{}` when every stage is
/// off, so future stages add keys inside the object without moving the
/// top-level schema.
#[derive(Serialize)]
struct ConditioningReport {
    #[serde(skip_serializing_if = "is_false")]
    dc_removal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    highpass: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agc: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limiter: Option<f32>,
}

fn is_false(value: &bool) -> bool {
    !value
}

pub fn run(args: CaptureArgs, json: bool, quiet: bool) -> Result<()> {
    // Pre-validate the device flags against the input device list so a
    // selector that matches nothing gives a helpful error before touching
    // the audio subsystem (exit 3, not 4).
    let selector = resolve_input(args.device.as_deref(), args.device_id.as_deref())?;
    let device_name = input_display_name(args.device.as_deref(), args.device_id.as_deref());

    let mut config = MicrophoneConfig::default();
    config.sample_rate = args.rate;
    config.channels = args.channels;
    config.frames_per_buffer = FRAMES_PER_BUFFER;
    config.device = selector;
    // Conditioning stages, each an independent opt-in. All four are pure DSP
    // in the library's capture chain; agc and limiter are honoured because
    // the `gain` feature is enabled, dc_removal and highpass need no feature.
    config.dc_removal = args.dc_removal;
    config.highpass = args.highpass.map(highpass_filter);
    config.agc = args.agc;
    config.limiter = args.limiter;

    let microphone = Microphone::new(config).map_err(|e| anyhow!("capture init failed: {e}"))?;

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
    // if called twice in the same process. Fine to ignore in tests/REPLs.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_handler = shutdown.clone();
    let _ = ctrlc::set_handler(move || {
        shutdown_handler.store(true, Ordering::SeqCst);
    });

    let stream = microphone
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

    // Interleaved samples per next_chunk block: frames times channels.
    let block_samples = FRAMES_PER_BUFFER as usize * args.channels as usize;

    let started = Instant::now();
    let mut samples_written: u64 = 0;
    let mut device_lost = false;
    let mut close_error: Option<DecibriError> = None;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        if let Some(d) = args.duration {
            if started.elapsed() >= d {
                break;
            }
        }

        match stream.next_chunk(block_samples, Some(POLL_INTERVAL)) {
            Ok(Some(chunk)) => {
                samples_written +=
                    write_chunk(&mut writer, &chunk).map_err(|e| exit::io(format!("{e:#}")))?;
                if let Some(pb) = &progress {
                    update_progress(pb, started.elapsed(), args.duration, samples_written);
                }
            }
            // Timeout with the stream still open: loop to re-check Ctrl+C
            // and duration expiry.
            Ok(None) => continue,
            // The stream ended without a local stop: a device or driver
            // failure. The library has already delivered every buffered
            // block and the final tail before reporting closed.
            Err(err) => {
                close_error = match err {
                    DecibriError::MicrophoneStreamClosed => None,
                    other => Some(other),
                };
                device_lost = true;
                break;
            }
        }
    }

    // Cooperative shutdown: stop the stream, then drain what the library
    // still holds. The closed path delivers remaining full blocks and the
    // final short tail before reporting `MicrophoneStreamClosed`.
    stream.stop();
    if !device_lost {
        let drain_deadline = Instant::now() + DRAIN_TIMEOUT;
        loop {
            match stream.next_chunk(block_samples, Some(POLL_INTERVAL)) {
                Ok(Some(chunk)) => {
                    samples_written +=
                        write_chunk(&mut writer, &chunk).map_err(|e| exit::io(format!("{e:#}")))?;
                }
                Ok(None) => {
                    if Instant::now() >= drain_deadline {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    let dropped_chunks = stream.overrun_count();
    // A driver failure recorded by the library takes precedence as the
    // device-loss cause; fall back to the error `next_chunk` surfaced.
    let close_cause = if device_lost {
        stream.take_last_error().or(close_error)
    } else {
        None
    };
    drop(stream);

    let elapsed = started.elapsed();
    let finalize_result = writer
        .finalize()
        .context("failed to finalize WAV")
        .map_err(|e| exit::io(format!("{e:#}")));

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    finalize_result?;

    if dropped_chunks > 0 {
        eprintln!(
            "warning: {dropped_chunks} capture buffer(s) dropped because the writer could not \
             keep up; the recording is missing that audio"
        );
    }

    if device_lost {
        let detail = close_cause.map(|e| format!(" ({e})")).unwrap_or_default();
        return Err(exit::io(format!(
            "audio device became unavailable during capture{detail}. \
             Partial recording preserved at {} ({:.1}s captured)",
            args.output.display(),
            elapsed.as_secs_f64()
        )));
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
            dropped_chunks,
            conditioning: ConditioningReport {
                dc_removal: args.dc_removal,
                highpass: args.highpass,
                agc: args.agc,
                limiter: args.limiter,
            },
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

/// Standard lossy f32 to i16 PCM conversion. Clamp protects against the rare
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_highpass_accepts_supported_cutoffs() {
        assert_eq!(parse_highpass("80"), Ok(80));
        assert_eq!(parse_highpass("100"), Ok(100));
    }

    #[test]
    fn parse_highpass_rejects_unsupported_values() {
        assert!(parse_highpass("0").is_err());
        assert!(parse_highpass("60").is_err());
        assert!(parse_highpass("300").is_err());
        assert!(parse_highpass("garbage").is_err());
        let msg = parse_highpass("60").unwrap_err();
        assert!(msg.contains("80, 100"), "unexpected message: {msg}");
    }

    #[test]
    fn parse_agc_accepts_range_bounds() {
        assert_eq!(parse_agc("-40"), Ok(-40));
        assert_eq!(parse_agc("-20"), Ok(-20));
        assert_eq!(parse_agc("-3"), Ok(-3));
    }

    #[test]
    fn parse_agc_rejects_out_of_range() {
        assert!(parse_agc("-41").is_err());
        assert!(parse_agc("-2").is_err());
        assert!(parse_agc("0").is_err());
        assert!(parse_agc("40").is_err());
        assert!(parse_agc("garbage").is_err());
        let msg = parse_agc("40").unwrap_err();
        assert!(msg.contains("[-40, -3]"), "unexpected message: {msg}");
    }

    #[test]
    fn parse_limiter_accepts_range_bounds() {
        assert_eq!(parse_limiter("-3.0"), Ok(-3.0));
        assert_eq!(parse_limiter("-1"), Ok(-1.0));
        assert_eq!(parse_limiter("-0.5"), Ok(-0.5));
        assert_eq!(parse_limiter("0"), Ok(0.0));
    }

    #[test]
    fn parse_limiter_rejects_out_of_range() {
        assert!(parse_limiter("-3.1").is_err());
        assert!(parse_limiter("0.1").is_err());
        assert!(parse_limiter("1").is_err());
        assert!(parse_limiter("NaN").is_err());
        assert!(parse_limiter("garbage").is_err());
        let msg = parse_limiter("1").unwrap_err();
        assert!(msg.contains("[-3.0, 0.0]"), "unexpected message: {msg}");
    }

    #[test]
    fn highpass_filter_maps_validated_cutoffs() {
        assert_eq!(highpass_filter(80), HighpassFilter::Hz80);
        assert_eq!(highpass_filter(100), HighpassFilter::Hz100);
    }

    // The conditioning object is always present, `{}` when every stage is
    // off, one key per active stage otherwise.
    #[test]
    fn conditioning_report_serializes_empty_when_all_off() {
        let report = ConditioningReport {
            dc_removal: false,
            highpass: None,
            agc: None,
            limiter: None,
        };
        assert_eq!(serde_json::to_string(&report).unwrap(), "{}");
    }

    #[test]
    fn conditioning_report_serializes_active_stages_only() {
        let report = ConditioningReport {
            dc_removal: true,
            highpass: Some(80),
            agc: None,
            limiter: Some(-1.0),
        };
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"dc_removal":true,"highpass":80,"limiter":-1.0}"#
        );
    }
}
