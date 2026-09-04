use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Args;
use decibri::{DecibriError, File, FileConfig, SaveFormat, SaveOptions};
use serde::Serialize;

use crate::conditioning::{
    highpass_filter, parse_agc, parse_highpass, parse_limiter, parse_rate, ConditioningReport,
};
use crate::exit;

// Process pipeline notes.
//
// The file is read whole, identified by content, decoded to f32, and handed
// to `File::buffer` with its own rate and channel count. `File::buffer`
// rather than `File::open` because the source rate has to be known before
// the config is built: `--rate` is optional, and omitting it keeps the
// source rate, which cannot be resolved from a path alone. Building from the
// decoded buffer also means the input format the JSON reports is the one the
// decoder actually used, not one inferred from an extension.
//
// The conditioning chain, the downmix to mono, and the resample to the
// target rate all happen inside `File`; `save` runs the single pass and
// reports what it did to the samples on the way into the container. Nothing
// here reimplements a stage the library already owns.
//
// The completion payload reports the output as it exists on disk, so the
// written file is read back and decoded rather than the sample count being
// predicted from the resampler's ratio. The read-back is exact by
// construction and, on a FLAC output, also runs that format's own MD5
// integrity check over what was just written.

#[derive(Args)]
pub struct ProcessArgs {
    /// Input audio file. Format is detected from the file's content: WAV,
    /// AIFF, AIFF-C, or FLAC.
    #[arg(long, short = 'i', value_name = "PATH")]
    pub input: PathBuf,

    /// Output audio file. The extension selects the container: .wav, .aiff,
    /// .aif, .aifc, or .flac. Always written as 16-bit mono.
    #[arg(long, short = 'o', value_name = "PATH", value_parser = parse_output)]
    pub output: PathBuf,

    /// Resample to this rate in Hz. Range: 1000 to 384000. If omitted, the
    /// source rate is kept.
    #[arg(long, short = 'r', value_name = "HZ", value_parser = parse_rate)]
    pub rate: Option<u32>,

    /// Remove a constant DC offset from the signal.
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

// The output container is named rather than sniffed, so an extension the
// library does not write is an argument error at parse time (exit 2), before
// the input is opened. The library's own resolver is the authority on the
// accepted set; this parser exists to move that answer earlier, not to
// restate it.
fn parse_output(s: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(s);
    match SaveFormat::from_path(&path) {
        Ok(_) => Ok(path),
        Err(e) => Err(e.to_string()),
    }
}

/// The CLI's name for a resolved save format. `SaveFormat` carries no
/// `Display`, and the JSON field is a contract, so the mapping is stated
/// here and pinned by a test rather than derived from the variant's `Debug`.
fn output_format_name(format: SaveFormat) -> &'static str {
    match format {
        SaveFormat::Wav => "WAV",
        SaveFormat::Aiff => "AIFF",
        SaveFormat::Flac => "FLAC",
        // `SaveFormat` is `#[non_exhaustive]`: a format added by a later
        // decibri cannot be named here, and reporting a wrong name is worse
        // than reporting that it is unknown. `parse_output` accepts only
        // extensions this build resolves, so this arm is not reachable from
        // the flag surface today.
        _ => "unknown",
    }
}

#[derive(Serialize)]
struct ProcessCompletion {
    input: String,
    output: String,
    input_format: String,
    output_format: String,
    duration_seconds: f64,
    sample_rate: u32,
    channels: u16,
    samples: u64,
    clipped_samples: u64,
    non_finite_samples: u64,
    conditioning: ConditioningReport,
}

/// Delivered channel count. `process` writes mono, matching `capture`.
const OUTPUT_CHANNELS: u16 = 1;

pub fn run(args: ProcessArgs, json: bool, quiet: bool) -> Result<()> {
    // Reading the file is IO; failing to read it is exit 4, matching
    // `capture` and `play`. What the bytes turn out to be is a separate
    // question, answered below.
    let bytes = fs::read(&args.input)
        .map_err(|e| exit::io(format!("failed to read {}: {e}", args.input.display())))?;

    // Content, never the extension: a `.wav` holding a FLAC stream decodes
    // as FLAC, and an unreadable container is named rather than guessed.
    let container = decibri_decode::identify(&bytes)
        .map_err(|e| anyhow!("cannot identify {}: {e}", args.input.display()))?;
    let audio = decibri_decode::decode(&bytes)
        .map_err(|e| anyhow!("cannot decode {}: {e}", args.input.display()))?;

    let source_rate = audio.sample_rate();
    let source_channels = audio.channels();
    // An omitted --rate keeps the source rate. Resolved here, before the
    // config is built, which is the reason this path goes through
    // `File::buffer` rather than `File::open`.
    let target_rate = args.rate.unwrap_or(source_rate);

    let mut config = FileConfig::default();
    config.sample_rate = target_rate;
    config.channels = OUTPUT_CHANNELS;
    // The same four opt-in stages `capture` exposes, over file samples
    // rather than device samples. agc and limiter are honoured because the
    // `gain` feature is enabled; dc_removal and highpass need no feature.
    config.dc_removal = args.dc_removal;
    config.highpass = args.highpass.map(highpass_filter);
    config.agc = args.agc;
    config.limiter = args.limiter;

    if !json && !quiet {
        eprintln!(
            "Processing {} ({container}, {source_rate} Hz, {source_channels} channel{})",
            args.input.display(),
            if source_channels == 1 { "" } else { "s" }
        );
    }

    let file = File::buffer(audio.into_samples(), source_rate, source_channels, config)
        .map_err(|e| anyhow!("cannot process {}: {e}", args.input.display()))?;

    let output_format = SaveFormat::from_path(&args.output)
        .map_err(|e| anyhow!("cannot resolve the output format: {e}"))?;

    let report = file
        .save(&args.output, SaveOptions::default())
        .map_err(|e| save_error(e, &args.output))?;

    // Report the file that exists, not the one that was predicted: decode
    // what was written and take the sample count from it.
    let written = fs::read(&args.output).map_err(|e| {
        exit::io(format!(
            "failed to read back {}: {e}",
            args.output.display()
        ))
    })?;
    let out_audio = decibri_decode::decode(&written).map_err(|e| {
        anyhow!(
            "cannot decode the file just written to {}: {e}",
            args.output.display()
        )
    })?;

    let samples = out_audio.samples().len() as u64;
    let out_channels = out_audio.channels();
    let out_rate = out_audio.sample_rate();
    let duration_seconds = if out_rate > 0 && out_channels > 0 {
        samples as f64 / (f64::from(out_rate) * f64::from(out_channels))
    } else {
        0.0
    };

    if json {
        let payload = ProcessCompletion {
            input: args.input.display().to_string(),
            output: args.output.display().to_string(),
            input_format: container.to_string(),
            output_format: output_format_name(output_format).to_string(),
            duration_seconds,
            sample_rate: target_rate,
            channels: OUTPUT_CHANNELS,
            samples,
            clipped_samples: report.clipped_samples,
            non_finite_samples: report.non_finite_samples,
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
            "Done. Wrote {} ({}, {} Hz, mono, {:.1}s, {samples} samples).",
            args.output.display(),
            output_format_name(output_format),
            target_rate,
            duration_seconds
        );
        if report.clipped_samples > 0 {
            eprintln!(
                "warning: {} sample(s) exceeded full scale and were clipped; \
                 lower the AGC target or add --limiter",
                report.clipped_samples
            );
        }
        if report.non_finite_samples > 0 {
            eprintln!(
                "warning: {} non-finite sample(s) in the conditioned signal were replaced",
                report.non_finite_samples
            );
        }
    }

    Ok(())
}

/// Route a save failure to its exit code: a write that could not reach the
/// disk is IO (exit 4), matching `capture` finalizing its WAV; anything else
/// is a generic failure (exit 1).
///
/// `FileWriteFailed` already names the path and the underlying cause, so it
/// is passed through as it stands. Every other variant is a chain or
/// configuration failure that does not name the destination, so the path is
/// added there.
fn save_error(err: DecibriError, output: &std::path::Path) -> anyhow::Error {
    match err {
        DecibriError::FileWriteFailed { .. } => exit::io(err.to_string()),
        other => anyhow!("failed to write {}: {other}", output.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The `output_format` JSON field is a contract; these are the strings it
    // carries. They match `decibri_decode::Container`'s `Display` so the
    // input and output format fields of one payload are directly comparable.
    #[test]
    fn output_format_names_are_stable() {
        assert_eq!(output_format_name(SaveFormat::Wav), "WAV");
        assert_eq!(output_format_name(SaveFormat::Aiff), "AIFF");
        assert_eq!(output_format_name(SaveFormat::Flac), "FLAC");
    }

    // Every extension the library writes is accepted by the flag parser, and
    // resolves to the format whose name the payload reports.
    #[test]
    fn parse_output_accepts_every_writable_extension() {
        for (path, expected) in [
            ("out.wav", "WAV"),
            ("out.WAV", "WAV"),
            ("out.aiff", "AIFF"),
            ("out.aif", "AIFF"),
            ("out.aifc", "AIFF"),
            ("out.flac", "FLAC"),
        ] {
            let parsed = parse_output(path).unwrap_or_else(|e| panic!("{path} rejected: {e}"));
            let format = SaveFormat::from_path(&parsed).expect("resolvable");
            assert_eq!(output_format_name(format), expected, "for {path}");
        }
    }

    #[test]
    fn parse_output_rejects_unwritable_extensions() {
        for bad in ["out.mp3", "out.ogg", "out"] {
            assert!(parse_output(bad).is_err(), "{bad} must be rejected");
        }
        let msg = parse_output("out.mp3").unwrap_err();
        assert!(msg.contains(".mp3"), "unexpected message: {msg}");
    }
}
