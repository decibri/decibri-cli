// Tests for `decibri process`.
//
// Unlike `capture` and `play`, `process` touches no audio device: it is a
// file in, file out command, so the whole path is exercised here rather than
// left to a manual hardware check. The tests write real inputs to a temp
// directory, run the binary over them, and read the results back, covering
// the argument surface, every exit code the command can produce, and the
// JSON completion schema.

use std::io::Cursor;
use std::process::Command;

use hound::{SampleFormat, WavSpec, WavWriter};
use tempfile::TempDir;

fn binary_path() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    });
    path.push(if cfg!(windows) {
        "decibri.exe"
    } else {
        "decibri"
    });
    path
}

/// Run the binary and return (exit code, stdout, stderr).
fn run(args: &[&std::ffi::OsStr]) -> (i32, String, String) {
    let output = Command::new(binary_path())
        .args(args)
        .output()
        .expect("failed to execute decibri binary; run `cargo build` first");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A deterministic 16-bit PCM WAV: a sine at `freq` on every channel, offset
/// by a constant so `--dc-removal` has something to remove.
fn write_wav(path: &std::path::Path, rate: u32, channels: u16, frames: usize, freq: f32) {
    let spec = WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("WavWriter::create");
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let s = (t * freq * std::f32::consts::TAU).sin() * 0.3;
        let q = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        for _ in 0..channels {
            writer.write_sample(q.saturating_add(1200)).unwrap();
        }
    }
    writer.finalize().expect("finalize");
}

#[test]
fn process_help_documents_all_flags() {
    let (code, stdout, stderr) = run(&["process".as_ref(), "--help".as_ref()]);
    assert_eq!(code, 0, "process --help failed: {stderr}");
    for flag in [
        "--input",
        "--output",
        "--rate",
        "--dc-removal",
        "--highpass",
        "--agc",
        "--limiter",
    ] {
        assert!(
            stdout.contains(flag),
            "process --help missing {flag}: {stdout}"
        );
    }
}

#[test]
fn top_level_help_lists_process() {
    let (code, stdout, _) = run(&["--help".as_ref()]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("process"),
        "--help missing process: {stdout}"
    );
}

#[test]
fn process_requires_input_and_output() {
    let (code, _, stderr) = run(&["process".as_ref()]);
    assert_eq!(code, 2, "expected exit 2 (invalid arguments)");
    assert!(
        stderr.contains("--input") || stderr.contains("required"),
        "expected required-arg error, got: {stderr}"
    );
}

// The output container is named by the extension, resolved at parse time.
// An extension decibri does not write is an argument error (exit 2), raised
// before the input file is even opened: the input path below does not exist,
// and the error is still about the extension.
#[test]
fn process_rejects_unwritable_output_extension() {
    let (code, _, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        "no_such_input_xyz.wav".as_ref(),
        "-o".as_ref(),
        "out.mp3".as_ref(),
    ]);
    assert_eq!(code, 2, "expected exit 2 (invalid arguments), got {code}");
    assert!(
        stderr.contains(".mp3"),
        "expected an error naming the extension, got: {stderr}"
    );
    assert!(
        stderr.contains(".wav"),
        "expected the accepted set in the message, got: {stderr}"
    );
}

// Every conditioning range and the sample rate are validated at the clap
// layer, so an out-of-range value is exit 2 rather than a library failure
// several steps later.
#[test]
fn process_rejects_out_of_range_values() {
    for (flag, bad, needle) in [
        ("--rate", "100", "[1000, 384000]"),
        ("--rate", "384001", "[1000, 384000]"),
        ("--highpass", "90", "80, 100"),
        ("--agc", "40", "[-40, -3]"),
        ("--agc", "-41", "[-40, -3]"),
        ("--limiter", "-5", "[-3.0, 0.0]"),
        ("--limiter", "1", "[-3.0, 0.0]"),
    ] {
        let (code, _, stderr) = run(&[
            "process".as_ref(),
            "-i".as_ref(),
            "in.wav".as_ref(),
            "-o".as_ref(),
            "out.wav".as_ref(),
            flag.as_ref(),
            bad.as_ref(),
        ]);
        assert_eq!(code, 2, "expected exit 2 for {flag} {bad}, got {code}");
        assert!(
            stderr.contains(needle),
            "expected {needle} in the error for {flag} {bad}, got: {stderr}"
        );
    }
}

#[test]
fn process_missing_input_exits_io_error() {
    let dir = TempDir::new().expect("tempdir");
    let out = dir.path().join("out.wav");
    let (code, _, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        "this_file_really_does_not_exist_xyz.wav".as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(code, 4, "expected exit 4 (IO error), got {code}");
    assert!(
        stderr.contains("failed to read"),
        "expected a read failure, got: {stderr}"
    );
}

// Bytes that are not a container decibri reads are a generic error (exit 1),
// matching `play`'s unsupported-format behaviour, not an IO error: the file
// was read successfully, it just is not audio.
#[test]
fn process_unidentifiable_input_exits_generic_error() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("junk.wav");
    let out = dir.path().join("out.wav");
    std::fs::write(&input, vec![0x7fu8; 512]).expect("write junk");

    let (code, _, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(
        code, 1,
        "expected exit 1 for unidentifiable input, got {code}"
    );
    assert!(
        stderr.contains("cannot identify"),
        "expected an identification failure, got: {stderr}"
    );
    assert!(!out.exists(), "no output should be written on failure");
}

// A truncated container identifies fine and fails in the decoder. Also exit
// 1: the bytes were read, they just do not decode.
#[test]
fn process_truncated_input_exits_generic_error() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("truncated.wav");
    let out = dir.path().join("out.wav");

    let mut buf: Vec<u8> = Vec::new();
    {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        for i in 0..1000i16 {
            w.write_sample(i).unwrap();
        }
        w.finalize().unwrap();
    }
    // Keep the header, drop most of the declared data chunk.
    buf.truncate(64);
    std::fs::write(&input, &buf).expect("write truncated");

    let (code, _, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(code, 1, "expected exit 1 for a truncated input, got {code}");
    assert!(
        stderr.contains("cannot decode") || stderr.contains("cannot identify"),
        "expected a decode failure, got: {stderr}"
    );
}

// The full path: a 48 kHz stereo source conditioned and resampled to 16 kHz
// mono. The output is decoded back to confirm the file on disk carries the
// rate and channel count the payload reports, and the payload itself is
// snapshotted to pin the JSON schema. Only the two path fields are redacted;
// every other value is deterministic, because decode, conditioning, and
// resampling are all bit-exact and platform-independent.
#[test]
fn process_json_completion_schema() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    let out = dir.path().join("out.wav");
    write_wav(&input, 48000, 2, 48000, 220.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "-r".as_ref(),
        "16000".as_ref(),
        "--dc-removal".as_ref(),
        "--highpass".as_ref(),
        "80".as_ref(),
        "--agc".as_ref(),
        "-20".as_ref(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "process failed: {stderr}");

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("process --json must be valid JSON");

    // The file on disk is what the payload describes.
    let written = std::fs::read(&out).expect("read output");
    let decoded = decibri_decode::decode(&written).expect("decode output");
    assert_eq!(decoded.sample_rate(), 16000, "output rate");
    assert_eq!(decoded.channels(), 1, "output is mono");
    assert_eq!(
        decoded.samples().len() as u64,
        parsed["samples"].as_u64().expect("samples"),
        "reported sample count must match the file"
    );

    insta::with_settings!({ sort_maps => true }, {
        insta::assert_json_snapshot!(parsed, {
            ".input" => "[input]",
            ".output" => "[output]",
        });
    });
}

// Omitting --rate keeps the source rate rather than falling back to a
// default.
#[test]
fn process_without_rate_keeps_source_rate() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    let out = dir.path().join("out.wav");
    write_wav(&input, 44100, 1, 4410, 440.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "process failed: {stderr}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["sample_rate"], 44100);
    assert_eq!(parsed["channels"], 1);
    // No rate change means no resampler, so the sample count is exactly the
    // source frame count.
    assert_eq!(parsed["samples"], 4410);

    let decoded = decibri_decode::decode(&std::fs::read(&out).unwrap()).expect("decode");
    assert_eq!(decoded.sample_rate(), 44100);
    assert_eq!(decoded.channels(), 1);
    assert_eq!(decoded.samples().len(), 4410);
}

// `conditioning` is always present and carries one key per active stage,
// `{}` when none are given.
#[test]
fn process_conditioning_is_empty_when_no_stage_is_active() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    let out = dir.path().join("out.wav");
    write_wav(&input, 16000, 1, 1600, 440.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "process failed: {stderr}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(
        parsed["conditioning"]
            .as_object()
            .expect("object")
            .is_empty(),
        "conditioning must be {{}} with no stages active: {parsed}"
    );
    assert_eq!(parsed["clipped_samples"], 0);
    assert_eq!(parsed["non_finite_samples"], 0);
}

// Every container decibri writes is reachable from the flag, and every
// container it reads is reachable as an input: a WAV goes out as FLAC, and
// that FLAC comes back in and goes out as AIFF, unchanged in rate, channel
// count, and sample count.
#[test]
fn process_round_trips_through_every_container() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    let flac = dir.path().join("out.flac");
    let aiff = dir.path().join("out.aiff");
    write_wav(&input, 16000, 1, 1600, 440.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        flac.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "wav to flac failed: {stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["input_format"], "WAV");
    assert_eq!(parsed["output_format"], "FLAC");
    assert_eq!(parsed["samples"], 1600);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        flac.as_os_str(),
        "-o".as_ref(),
        aiff.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "flac to aiff failed: {stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["input_format"], "FLAC");
    assert_eq!(parsed["output_format"], "AIFF");
    assert_eq!(parsed["sample_rate"], 16000);
    assert_eq!(parsed["samples"], 1600);

    let decoded = decibri_decode::decode(&std::fs::read(&aiff).unwrap()).expect("decode aiff");
    assert_eq!(decoded.sample_rate(), 16000);
    assert_eq!(decoded.channels(), 1);
    assert_eq!(decoded.samples().len(), 1600);
}

// A multichannel source is downmixed rather than refused: the delivered
// channel count is always 1.
#[test]
fn process_downmixes_multichannel_input_to_mono() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("stereo.wav");
    let out = dir.path().join("mono.wav");
    write_wav(&input, 16000, 2, 1600, 440.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(code, 0, "process failed: {stderr}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["channels"], 1);
    assert_eq!(parsed["samples"], 1600, "one sample per source frame");

    let decoded = decibri_decode::decode(&std::fs::read(&out).unwrap()).expect("decode");
    assert_eq!(decoded.channels(), 1);
}

// The `--quiet` global suppresses the human progress lines without changing
// the exit code or the file written.
#[test]
fn process_quiet_suppresses_human_output() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    let out = dir.path().join("out.wav");
    write_wav(&input, 16000, 1, 1600, 440.0);

    let (code, stdout, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "--quiet".as_ref(),
    ]);
    assert_eq!(code, 0, "process failed: {stderr}");
    assert!(stdout.is_empty(), "quiet must print nothing to stdout");
    assert!(stderr.is_empty(), "quiet must print nothing to stderr");
    assert!(out.exists(), "the output file is still written");
}

// A destination that cannot be written is an IO error (exit 4), matching
// `capture` failing to finalize its WAV. The input is read and decoded
// first, so this exercises the save path rather than an early rejection.
#[test]
fn process_unwritable_output_exits_io_error() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("source.wav");
    // A directory that does not exist: the encoder cannot create the file.
    let out = dir
        .path()
        .join("no")
        .join("such")
        .join("dir")
        .join("out.wav");
    write_wav(&input, 16000, 1, 1600, 440.0);

    let (code, _, stderr) = run(&[
        "process".as_ref(),
        "-i".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        out.as_os_str(),
        "--quiet".as_ref(),
    ]);
    assert_eq!(code, 4, "expected exit 4 (IO error), got {code}");
    assert!(
        stderr.to_lowercase().contains("write"),
        "expected a write failure, got: {stderr}"
    );
}
