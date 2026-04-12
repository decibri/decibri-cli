use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use console::Term;
use decibri::device::DeviceSelector;
use decibri::output::{AudioOutput, OutputConfig};
use hound::{SampleFormat, WavReader};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::device_resolve::{output_display_name, resolve_output_selector};
use crate::exit;

/// Number of samples (across all channels) we ship to the library per `send()`
/// call. 4096 interleaved samples is ~256ms at 16 kHz mono or ~46ms at
/// 44.1 kHz stereo — small enough to keep Ctrl+C latency low (we only check
/// the shutdown flag between sends), large enough to keep channel/syscall
/// overhead negligible. The library's internal channel is bounded at 32, so
/// the maximum in-flight backlog is ~8s of voice audio.
const FEED_CHUNK_SAMPLES: usize = 4096;

#[derive(Args)]
pub struct PlayArgs {
    /// WAV file to play.
    pub file: PathBuf,

    /// Device name substring (case-insensitive) or numeric index from `decibri devices`.
    #[arg(long)]
    pub device: Option<String>,
}

#[derive(Serialize)]
struct PlayCompletion {
    file: String,
    duration_seconds: f64,
    sample_rate: u32,
    channels: u16,
    samples: u64,
    device: String,
    interrupted: bool,
}

pub fn run(args: PlayArgs, json: bool, quiet: bool) -> Result<()> {
    // Open the WAV file. File-not-found and permission errors map to exit 4.
    let reader = WavReader::open(&args.file)
        .with_context(|| format!("failed to open {}", args.file.display()))
        .map_err(|e| exit::io(format!("{e:#}")))?;
    let spec = reader.spec();

    // Only 16-bit PCM int and 32-bit float WAVs are supported in v0.1.0.
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) | (SampleFormat::Float, 32) => {}
        (fmt, bits) => {
            return Err(anyhow!(
                "unsupported WAV format: {fmt:?} {bits}-bit. v0.1.0 supports 16-bit PCM int and 32-bit float only."
            ));
        }
    }

    let samples = load_samples(reader, spec).map_err(|e| exit::io(format!("{e:#}")))?;
    let total_samples = samples.len() as u64;
    let duration_seconds = samples_to_seconds(total_samples, spec.sample_rate, spec.channels);

    // Resolve device (exit 3 on no match, before touching the audio subsystem).
    let selector = match &args.device {
        Some(s) => resolve_output_selector(s)?,
        None => DeviceSelector::Default,
    };
    let device_name = output_display_name(args.device.as_deref());

    let config = OutputConfig {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        device: selector,
    };

    let output = AudioOutput::new(config).map_err(|e| anyhow!("output init failed: {e}"))?;

    // ctrlc handler: flip an AtomicBool observed by the feed loop.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_handler = shutdown.clone();
    let _ = ctrlc::set_handler(move || {
        shutdown_handler.store(true, Ordering::SeqCst);
    });

    let stream = output
        .start()
        .map_err(|e| exit::io(format!("output start failed: {e}")))?;

    if !json && !quiet {
        let fmt_name = match spec.sample_format {
            SampleFormat::Int => format!("{}-bit PCM WAV", spec.bits_per_sample),
            SampleFormat::Float => format!("{}-bit float WAV", spec.bits_per_sample),
        };
        eprintln!(
            "Playing {} ({} Hz, {} channel{}, {})",
            args.file.display(),
            spec.sample_rate,
            spec.channels,
            if spec.channels == 1 { "" } else { "s" },
            fmt_name
        );
        eprintln!("Device: {device_name}");
    }

    let show_progress = !quiet && !json && Term::stderr().features().is_attended();
    let progress = if show_progress {
        Some(make_progress_bar(total_samples))
    } else {
        None
    };

    // Feed loop. `stream.send()` blocks on backpressure against the library's
    // bounded channel (capacity 32), so this naturally throttles to the audio
    // playback rate — no sleep, no manual pacing. We only check the shutdown
    // flag between sends, so worst-case Ctrl+C latency is one chunk-time.
    let mut fed: u64 = 0;
    let mut interrupted = false;
    for chunk in samples.chunks(FEED_CHUNK_SAMPLES) {
        if shutdown.load(Ordering::SeqCst) {
            interrupted = true;
            break;
        }
        stream
            .send(chunk.to_vec())
            .map_err(|e| exit::io(format!("output send failed: {e}")))?;
        fed += chunk.len() as u64;
        if let Some(pb) = &progress {
            pb.set_position(fed);
        }
    }

    // Finish: drain on normal EOF (blocks until audio fully played out), stop
    // on Ctrl+C (discards pending samples — user wants silence NOW).
    if interrupted {
        stream.stop();
    } else {
        stream.drain();
    }
    drop(stream);

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    let played = if interrupted { fed } else { total_samples };
    let played_duration = samples_to_seconds(played, spec.sample_rate, spec.channels);

    if json {
        let payload = PlayCompletion {
            file: args.file.display().to_string(),
            duration_seconds: played_duration,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            samples: played,
            device: device_name,
            interrupted,
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !quiet {
        let verb = if interrupted { "Interrupted" } else { "Done" };
        eprintln!("{verb}. {played_duration:.1}s played ({played} samples).");
    }

    let _ = duration_seconds; // suppress unused warning when not in JSON branch
    Ok(())
}

/// Load an entire WAV file into a Vec<f32> for interleaved samples. We do the
/// i16 → f32 conversion here; the decibri output API takes f32 interleaved.
/// Reading the whole file up front is fine for v0.1.0 — a 1-hour 16 kHz mono
/// recording is ~230 MB of f32, and playback is a one-shot operation, not a
/// streaming decode.
fn load_samples(mut reader: WavReader<BufReader<File>>, spec: hound::WavSpec) -> Result<Vec<f32>> {
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|r| r.map(i16_to_f32).map_err(anyhow::Error::from))
            .collect(),
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|r| r.map_err(anyhow::Error::from))
            .collect(),
        _ => unreachable!("format validated earlier"),
    }
}

/// i16 → f32 conversion for one sample. i16::MAX → 1.0, i16::MIN → slightly
/// under -1.0 (standard lossy round-trip; not a bug). The decibri output API
/// expects f32 samples in [-1.0, 1.0].
fn i16_to_f32(sample: i16) -> f32 {
    f32::from(sample) / f32::from(i16::MAX)
}

fn samples_to_seconds(samples: u64, sample_rate: u32, channels: u16) -> f64 {
    if sample_rate == 0 || channels == 0 {
        return 0.0;
    }
    samples as f64 / (sample_rate as f64 * channels as f64)
}

fn make_progress_bar(total_samples: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_samples);
    pb.set_style(
        ProgressStyle::with_template(
            "Playing: [{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len} samples",
        )
        .expect("indicatif template")
        .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_to_f32_boundary_values() {
        assert!((i16_to_f32(0) - 0.0).abs() < 1e-9);
        assert!((i16_to_f32(i16::MAX) - 1.0).abs() < 1e-9);
        // i16::MIN is -32768, scaled by 1/32767, produces slightly less than -1.0.
        // This is the standard lossy round-trip; not a bug.
        let min = i16_to_f32(i16::MIN);
        assert!(min <= -1.0 && min > -1.001, "unexpected min: {min}");
    }

    #[test]
    fn samples_to_seconds_basic() {
        // 16000 samples mono 16kHz = 1.0s
        assert!((samples_to_seconds(16000, 16000, 1) - 1.0).abs() < 1e-9);
        // 88200 samples stereo 44.1kHz = 1.0s
        assert!((samples_to_seconds(88200, 44100, 2) - 1.0).abs() < 1e-9);
        // zero-guard
        assert_eq!(samples_to_seconds(100, 0, 1), 0.0);
        assert_eq!(samples_to_seconds(100, 16000, 0), 0.0);
    }
}
