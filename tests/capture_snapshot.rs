// Hardware-independent tests for `decibri capture`.
//
// Real audio capture is verified by the user manually per BUILD-PLAN's
// hardware-test policy. CI runs only the binary-level argument validation
// and a hound round-trip that simulates the synthetic-PCM → WAV → read-back
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
        .expect("failed to execute decibri binary — run `cargo build` first");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for flag in ["--output", "--duration", "--rate", "--channels", "--device"] {
        assert!(
            stdout.contains(flag),
            "capture --help missing {flag}: {stdout}"
        );
    }
    // v0.2.0 flags must NOT have leaked into v0.1.0.
    assert!(
        !stdout.contains("--vad"),
        "--vad must not appear in v0.1.0: {stdout}"
    );
    assert!(
        !stdout.contains("--silence-ms"),
        "--silence-ms must not appear in v0.1.0: {stdout}"
    );
    assert!(
        !stdout.contains("--raw"),
        "--raw must not appear in v0.1.0: {stdout}"
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

// An empty WAV (zero samples) must still be valid — the user's "zero chunks
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
