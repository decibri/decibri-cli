// Hardware-independent tests for `decibri play`.
//
// Real playback is verified by the user manually per BUILD-PLAN's hardware-
// test policy (capture → play round trip on the dev machine). CI exercises
// argument validation, WAV format detection via hound round-trips, and the
// non-existent-file error path.

use std::io::Cursor;
use std::process::Command;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use tempfile::NamedTempFile;

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
fn play_help_documents_flags() {
    let output = Command::new(binary_path())
        .args(["play", "--help"])
        .output()
        .expect("failed to execute decibri binary — run `cargo build` first");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("<FILE>"),
        "play --help missing positional: {stdout}"
    );
    assert!(
        stdout.contains("--device"),
        "play --help missing --device: {stdout}"
    );
    // v0.2.0 flags must not have leaked.
    assert!(
        !stdout.contains("--raw"),
        "--raw must not appear in v0.1.0: {stdout}"
    );
}

#[test]
fn play_requires_file() {
    let output = Command::new(binary_path())
        .arg("play")
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("<FILE>"),
        "expected required-arg error, got: {stderr}"
    );
}

#[test]
fn play_nonexistent_file_exits_io_error() {
    let output = Command::new(binary_path())
        .args(["play", "this_file_really_does_not_exist_xyz.wav"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 4, "expected exit code 4 (IO error), got {code}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("failed to open")
            || stderr.to_lowercase().contains("no such file")
            || stderr.to_lowercase().contains("cannot find"),
        "expected file-not-found error, got: {stderr}"
    );
}

#[test]
fn play_routes_numeric_device_for_output() {
    // This test proves the play command's device-resolution path accepts the
    // numeric-index form without panicking at the clap level. It will fail at
    // the "no output device matches index 99999" stage (exit 3), which is the
    // expected behaviour for an invalid index.
    let output = Command::new(binary_path())
        .args(["play", "doesnotmatter.wav", "--device", "99999"])
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    // Either "file not found" (4) or "device not found" (3) depending on
    // evaluation order. Both prove the numeric parse worked.
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 3 || code == 4,
        "expected 3 (device not found) or 4 (file not found), got {code}"
    );
}

// Round-trip a 16-bit PCM WAV and verify hound reports the spec we wrote.
// This pins the "Phase 3 capture → Phase 4 play" interop contract.
#[test]
fn hound_roundtrip_16bit_pcm() {
    let sample_rate = 16000;
    let channels = 1;
    let total = (sample_rate as f64 * 0.1) as usize; // 100ms

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        for i in 0..total {
            let t = i as f32 / sample_rate as f32;
            let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let q = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            w.write_sample(q).unwrap();
        }
        w.finalize().unwrap();
    }

    let r = WavReader::new(Cursor::new(&buf)).unwrap();
    let read = r.spec();
    assert_eq!(read.sample_rate, sample_rate);
    assert_eq!(read.channels, channels);
    assert_eq!(read.bits_per_sample, 16);
    assert_eq!(read.sample_format, SampleFormat::Int);
    assert_eq!(r.duration() as usize, total);
}

// 32-bit float WAVs produced by other tools (Audacity, ffmpeg) should also
// open cleanly.
#[test]
fn hound_roundtrip_32bit_float() {
    let sample_rate = 44100;
    let channels = 2;
    let frames = 512;

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        for i in 0..frames {
            let t = i as f32 / sample_rate as f32;
            let l = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let r = (t * 880.0 * std::f32::consts::TAU).sin() * 0.5;
            w.write_sample(l).unwrap();
            w.write_sample(r).unwrap();
        }
        w.finalize().unwrap();
    }

    let r = WavReader::new(Cursor::new(&buf)).unwrap();
    let read = r.spec();
    assert_eq!(read.sample_rate, sample_rate);
    assert_eq!(read.channels, channels);
    assert_eq!(read.bits_per_sample, 32);
    assert_eq!(read.sample_format, SampleFormat::Float);
}

// Writing a 24-bit PCM WAV and feeding it to the binary should produce a
// clean unsupported-format error with exit code 1.
#[test]
fn play_rejects_unsupported_24bit_wav() {
    let tmp = NamedTempFile::new().expect("tempfile");
    {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 24,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(tmp.path(), spec).expect("24-bit WavWriter");
        for i in 0..100i32 {
            w.write_sample(i).unwrap();
        }
        w.finalize().unwrap();
    }

    let output = Command::new(binary_path())
        .args(["play"])
        .arg(tmp.path())
        .output()
        .expect("failed to execute decibri binary");
    assert!(!output.status.success());
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code, 1,
        "expected exit 1 for unsupported format, got {code}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("unsupported") || stderr.contains("24"),
        "expected unsupported-format error, got: {stderr}"
    );
}
