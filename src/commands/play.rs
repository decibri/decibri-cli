use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use console::Term;
use decibri::{Speaker, SpeakerConfig};
use hound::{SampleFormat, WavReader};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::device_resolve::{output_display_name, resolve_output};
use crate::exit;

/// Number of samples (across all channels) we ship to the library per `send()`
/// call. 4096 interleaved samples is ~256ms at 16 kHz mono or ~46ms at
/// 44.1 kHz stereo: large enough to keep channel/syscall overhead negligible,
/// small enough for smooth progress updates. Interrupt latency does not depend
/// on the chunk size: Ctrl+C stops the stream, which immediately unblocks even
/// a send parked on backpressure, so teardown is bounded by stream shutdown,
/// not by a chunk time. The library's internal channel is bounded at 32, so
/// the maximum in-flight backlog is ~8s of voice audio.
const FEED_CHUNK_SAMPLES: usize = 4096;

#[derive(Args)]
pub struct PlayArgs {
    /// WAV file to play.
    pub file: PathBuf,

    /// Device name substring (case-insensitive) or numeric index from `decibri devices`.
    #[arg(long)]
    pub device: Option<String>,

    /// Stable device ID from `decibri devices --json` (exact match).
    #[arg(long, conflicts_with = "device")]
    pub device_id: Option<String>,
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

    // Only 16-bit PCM int and 32-bit float WAVs are supported.
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) | (SampleFormat::Float, 32) => {}
        (fmt, bits) => {
            return Err(anyhow!(
                "unsupported WAV format: {fmt:?} {bits}-bit. Supported inputs are 16-bit PCM int and 32-bit float."
            ));
        }
    }

    let samples = load_samples(reader, spec).map_err(|e| exit::io(format!("{e:#}")))?;
    let total_samples = samples.len() as u64;
    let duration_seconds = samples_to_seconds(total_samples, spec.sample_rate, spec.channels);

    // Resolve device (exit 3 on no match, before touching the audio subsystem).
    let selector = resolve_output(args.device.as_deref(), args.device_id.as_deref())?;
    let device_name = output_display_name(args.device.as_deref(), args.device_id.as_deref());

    let mut config = SpeakerConfig::default();
    config.sample_rate = spec.sample_rate;
    config.channels = spec.channels;
    config.device = selector;

    let output = Speaker::new(config).map_err(|e| anyhow!("output init failed: {e}"))?;

    let stream = Arc::new(
        output
            .start()
            .map_err(|e| exit::io(format!("output start failed: {e}")))?,
    );

    // Ctrl+C: flip the shutdown flag AND stop the stream. Stopping releases
    // the device and disconnects the playback channel, which unblocks the
    // feeder thread even when it is parked inside a backpressured send or
    // drain. The handler holds its own Arc clone of the stream; stop() is
    // idempotent.
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let shutdown_handler = shutdown.clone();
        let stream_handler = stream.clone();
        let _ = ctrlc::set_handler(move || {
            shutdown_handler.store(true, Ordering::SeqCst);
            stream_handler.stop();
        });
    }

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

    // Feeder thread. It owns a SpeakerSink (a Send + Sync + Clone handle to
    // the playback channel) and runs the blocking send loop off the main
    // thread, so the main thread stays free to orchestrate shutdown. A parked
    // send is released when the stream is stopped (by the Ctrl+C handler or
    // by the device-death handling below): send() then returns
    // SpeakerStreamClosed and the thread ends. On normal EOF the feeder also
    // drains the queued tail here, on this thread, so the main thread keeps
    // watching the stream during the drain and a device lost at any point
    // (including while drain itself is parked on the full channel) is still
    // observed and released. `fed` is tracked in a shared atomic so the
    // played-sample count survives regardless of how the thread ends.
    let sink = stream.sink();
    let fed_shared = Arc::new(AtomicU64::new(0));
    let feeder = {
        let shutdown = shutdown.clone();
        let progress = progress.clone();
        let fed_shared = fed_shared.clone();
        thread::spawn(move || {
            for chunk in samples.chunks(FEED_CHUNK_SAMPLES) {
                if shutdown.load(Ordering::SeqCst) {
                    return;
                }
                match sink.send(chunk.to_vec()) {
                    Ok(()) => {
                        let fed = fed_shared.fetch_add(chunk.len() as u64, Ordering::Relaxed)
                            + chunk.len() as u64;
                        if let Some(pb) = &progress {
                            pb.set_position(fed);
                        }
                    }
                    Err(_) => return,
                }
            }
            // Normal EOF: block until the queued tail has played. Returns
            // early if the stream is stopped or fails in the meantime.
            sink.drain();
        })
    };

    // Orchestration. Wait for the feeder while watching for a device failure.
    // The feeder finishes on normal EOF (after draining the tail), when a
    // send returns closed after a stop(), or when it sees the shutdown flag.
    // Device death flips is_playing() to false via the library's error
    // callback but does not by itself unblock a parked send or drain, so the
    // main thread observes it and calls stop(), which disconnects the channel,
    // releases the device, and ends the feeder.
    while !feeder.is_finished() {
        if !stream.is_playing() {
            stream.stop();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = feeder.join();
    let fed = fed_shared.load(Ordering::Relaxed);

    let interrupted = shutdown.load(Ordering::SeqCst);

    // Release the device in every path. On Ctrl+C the handler already stopped
    // the stream, and on device death the loop above did; stop() is
    // idempotent. On normal EOF the feeder has drained the queued tail, so
    // nothing audible is discarded.
    stream.stop();
    let playback_error = stream.take_last_error();

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    if let Some(err) = playback_error {
        return Err(exit::io(format!(
            "output device became unavailable during playback ({err})"
        )));
    }

    let played = played_samples(interrupted, fed, total_samples);
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
/// i16 to f32 conversion here; the decibri speaker API takes f32 interleaved.
/// Reading the whole file up front keeps playback simple: a 1-hour 16 kHz
/// mono recording is ~230 MB of f32, and playback is a one-shot operation,
/// not a streaming decode.
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

/// i16 to f32 conversion for one sample. i16::MAX maps to 1.0, i16::MIN to
/// slightly under -1.0 (standard lossy round-trip; not a bug). The decibri
/// speaker API expects f32 samples in [-1.0, 1.0].
fn i16_to_f32(sample: i16) -> f32 {
    f32::from(sample) / f32::from(i16::MAX)
}

/// Samples reported as played. An interrupted run reports the fed count (what
/// reached the playback queue before the stop); a normal completion drained
/// the queue, so the whole file played.
fn played_samples(interrupted: bool, fed: u64, total_samples: u64) -> u64 {
    if interrupted {
        fed
    } else {
        total_samples
    }
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
    fn played_samples_selection() {
        // Interrupted: report what was fed before the stop.
        assert_eq!(played_samples(true, 4096, 16000), 4096);
        // Normal completion: the drain played everything, report the full file.
        assert_eq!(played_samples(false, 4096, 16000), 16000);
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
