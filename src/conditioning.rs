// Conditioning flags shared by `capture` and `process`.
//
// Both commands expose the same four opt-in conditioning stages and the same
// sample-rate flag, and both report the active stages the same way in JSON.
// The value parsers, the closed-set cutoff mapping, and the report shape live
// here so there is one statement of each rather than one per command: a range
// that drifted between the two would be a silent contract difference, and the
// high-pass mapping duplicated into a second file is the kind of closed set
// that gets extended in one copy only.
//
// The ranges are the library's own. `parse_rate` mirrors decibri's
// 1000-384000 window (`DecibriError::SampleRateOutOfRange`), `parse_agc` its
// -40..=-3 AGC target, and `parse_limiter` its -3.0..=0.0 ceiling, so an
// out-of-range value is an argument error (exit 2) at parse time rather than
// a library error (exit 1) several steps later.

use decibri::HighpassFilter;
use serde::Serialize;

/// Sample rate in Hz, validated against the library's supported window.
pub(crate) fn parse_rate(s: &str) -> std::result::Result<u32, String> {
    match s.parse::<u32>() {
        Ok(rate) if (1000..=384_000).contains(&rate) => Ok(rate),
        Ok(rate) => Err(format!("rate must be in [1000, 384000] Hz; got {rate}")),
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
pub(crate) fn highpass_filter(hz: u32) -> HighpassFilter {
    match hz {
        80 => HighpassFilter::Hz80,
        100 => HighpassFilter::Hz100,
        other => unreachable!("parse_highpass admits only 80 and 100, got {other}"),
    }
}

/// The conditioning stages active for one run, one key per active stage.
/// Always present in a completion payload, `{}` when every stage is off, so
/// future stages add keys inside the object without moving the top-level
/// schema.
#[derive(Serialize)]
pub(crate) struct ConditioningReport {
    #[serde(skip_serializing_if = "is_false")]
    pub dc_removal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highpass: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agc: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limiter: Option<f32>,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_accepts_range_bounds() {
        assert_eq!(parse_rate("1000"), Ok(1000));
        assert_eq!(parse_rate("16000"), Ok(16000));
        assert_eq!(parse_rate("44100"), Ok(44100));
        assert_eq!(parse_rate("384000"), Ok(384_000));
    }

    #[test]
    fn parse_rate_rejects_out_of_range() {
        assert!(parse_rate("999").is_err());
        assert!(parse_rate("384001").is_err());
        assert!(parse_rate("0").is_err());
        assert!(parse_rate("-1").is_err());
        assert!(parse_rate("garbage").is_err());
        let msg = parse_rate("100").unwrap_err();
        assert!(msg.contains("[1000, 384000]"), "unexpected message: {msg}");
    }

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
