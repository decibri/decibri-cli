// Hardware-independent tests for `decibri capture`.
//
// Real audio capture is verified manually on real hardware; CI runners have
// no audio devices. CI runs only the binary-level argument validation and a
// hound round-trip that simulates the synthetic-PCM to WAV to read-back
// path that the capture command exercises internally.

use std::io::Cursor;
use std::process::Command;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

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

#[test]
fn capture_help_documents_all_flags() {
    let output = Command::new(binary_path())
        .args(["capture", "--help"])
        .output()
        .expect("failed to execute decibri binary; run `cargo build` first");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for flag in [
        "--output",
        "--duration",
        "--rate",
        "--channels",
        "--device",
        "--device-id",
        "--dc-removal",
        "--highpass",
        "--agc",
        "--limiter",
    ] {
        assert!(
            stdout.contains(flag),
            "capture --help missing {flag}: {stdout}"
        );
    }
}

// Capture is mono only: any --channels value other than 1 is rejected at the
// clap layer with exit code 2 (invalid arguments), before the audio
// subsystem is touched.
#[test]
fn capture_rejects_multichannel() {
    let output = Command::new(binary_path())
        .args(["capture", "-o", "x.wav", "-c", "2"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "--channels 2 must error");
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 2, "expected exit 2 (invalid arguments), got {code}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mono"),
        "expected mono-only arg error, got: {stderr}"
    );
}

// --device-id resolution runs before the output file is created. A
// nonexistent ID exits 3 (device not found) on hosts where enumeration
// works, or 4 when the audio subsystem itself is unavailable (headless CI).
// Either way this proves clap accepts the flag, the explicit `-c 1` value
// passes the mono-only parser, and the ID pre-validation path executes.
#[test]
fn capture_device_id_no_match_is_device_not_found() {
    let output = Command::new(binary_path())
        .args([
            "capture",
            "-o",
            "x.wav",
            "-c",
            "1",
            "--device-id",
            "no-such-device-id-zzz",
        ])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "nonexistent device ID must error");
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 3 || code == 4,
        "expected 3 (device not found) or 4 (audio subsystem unavailable), got {code}"
    );
}

// --device and --device-id are mutually exclusive; clap rejects supplying
// both with exit code 2.
#[test]
fn capture_rejects_device_and_device_id_together() {
    let output = Command::new(binary_path())
        .args([
            "capture",
            "-o",
            "x.wav",
            "--device",
            "mic",
            "--device-id",
            "some-id",
        ])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "conflicting flags must error");
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 2, "expected exit 2 (invalid arguments), got {code}");
}

// The high-pass cutoff is a closed set (80, 100), matching the library's
// named-cutoff selector. Anything else is rejected at the clap layer with
// exit code 2 and a message naming the supported cutoffs.
#[test]
fn capture_rejects_unsupported_highpass_cutoff() {
    let output = Command::new(binary_path())
        .args(["capture", "-o", "x.wav", "--highpass", "60"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "--highpass 60 must error");
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 2, "expected exit 2 (invalid arguments), got {code}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("80, 100"),
        "expected supported-cutoff arg error, got: {stderr}"
    );
}

// The AGC target must fall in [-40, -3] dBFS; an out-of-range value is
// rejected at the clap layer with exit code 2.
#[test]
fn capture_rejects_out_of_range_agc() {
    for bad in ["40", "-41", "0"] {
        let output = Command::new(binary_path())
            .args(["capture", "-o", "x.wav", "--agc", bad])
            .output()
            .expect("failed to execute decibri binary");
        assert!(!output.status.success(), "--agc {bad} must error");
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(
            code, 2,
            "expected exit 2 (invalid arguments) for --agc {bad}, got {code}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("[-40, -3]"),
            "expected agc range arg error for --agc {bad}, got: {stderr}"
        );
    }
}

// The limiter ceiling must fall in [-3.0, 0.0] dBFS; an out-of-range value
// is rejected at the clap layer with exit code 2.
#[test]
fn capture_rejects_out_of_range_limiter() {
    for bad in ["1", "-3.1", "0.5"] {
        let output = Command::new(binary_path())
            .args(["capture", "-o", "x.wav", "--limiter", bad])
            .output()
            .expect("failed to execute decibri binary");
        assert!(!output.status.success(), "--limiter {bad} must error");
        let code = output.status.code().unwrap_or(-1);
        assert_eq!(
            code, 2,
            "expected exit 2 (invalid arguments) for --limiter {bad}, got {code}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("[-3.0, 0.0]"),
            "expected limiter range arg error for --limiter {bad}, got: {stderr}"
        );
    }
}

// Negative conditioning values (`--agc -20`, `--limiter -1`) must parse as
// option values, not be mistaken for flags. Combined with a nonexistent
// device ID the run gets past clap (which would exit 2) and fails at device
// resolution: 3 where enumeration works, 4 where the audio subsystem is
// unavailable (headless CI). Same pattern as the --device-id test above.
#[test]
fn capture_accepts_negative_conditioning_values() {
    let output = Command::new(binary_path())
        .args([
            "capture",
            "-o",
            "x.wav",
            "--dc-removal",
            "--highpass",
            "80",
            "--agc",
            "-20",
            "--limiter",
            "-1",
            "--device-id",
            "no-such-device-id-zzz",
        ])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "nonexistent device ID must error");
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 3 || code == 4,
        "expected 3 (device not found) or 4 (audio subsystem unavailable), got {code}"
    );
}

#[test]
fn capture_requires_output() {
    let output = Command::new(binary_path())
        .arg("capture")
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--output") || stderr.contains("required"),
        "expected required-arg error, got: {stderr}"
    );
}

#[test]
fn capture_rejects_invalid_duration() {
    let output = Command::new(binary_path())
        .args(["capture", "-o", "x.wav", "-d", "garbage"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("duration"),
        "expected duration parse error, got: {stderr}"
    );
}

#[test]
fn capture_rejects_negative_duration() {
    let output = Command::new(binary_path())
        .args(["capture", "-o", "x.wav", "-d", "-1"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success(), "negative duration must error");
}

// Round-trip a synthetic f32 → i16 → WAV → read-back to verify the conversion
// path the capture command uses. Sine wave at known frequency makes drift
// detectable; we just check the header and sample count match.
#[test]
fn synthetic_f32_to_wav_roundtrip() {
    let sample_rate: u32 = 16000;
    let channels: u16 = 1;
    let duration_secs: f64 = 0.25;
    let total_samples = (sample_rate as f64 * duration_secs) as usize;

    let f32_samples: Vec<f32> = (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
        })
        .collect();

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = WavWriter::new(cursor, spec).expect("WavWriter::new");
        for &s in &f32_samples {
            let i = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            writer.write_sample(i).expect("write_sample");
        }
        writer.finalize().expect("finalize");
    }

    assert!(buf.starts_with(b"RIFF"), "missing RIFF header");
    assert!(&buf[8..12] == b"WAVE", "missing WAVE marker");

    let reader = WavReader::new(Cursor::new(&buf)).expect("WavReader::new");
    let read_spec = reader.spec();
    assert_eq!(read_spec.sample_rate, sample_rate);
    assert_eq!(read_spec.channels, channels);
    assert_eq!(read_spec.bits_per_sample, 16);
    assert_eq!(read_spec.sample_format, SampleFormat::Int);

    let read_samples: Vec<i16> = reader
        .into_samples::<i16>()
        .map(|r| r.expect("read sample"))
        .collect();
    assert_eq!(read_samples.len(), total_samples);
}

// An empty WAV (zero samples) must still be valid: the "zero chunks
// captured" branch must produce a file VLC and Audacity can open. We don't
// have hound here in the binary, so simulate the same finalize() path.
#[test]
fn empty_wav_finalizes_cleanly() {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let writer = WavWriter::new(cursor, spec).expect("WavWriter::new");
        writer.finalize().expect("finalize empty");
    }
    let reader = WavReader::new(Cursor::new(&buf)).expect("read empty WAV");
    assert_eq!(reader.duration(), 0);
    assert_eq!(reader.spec().sample_rate, 16000);
}
