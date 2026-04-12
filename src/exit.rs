// Marker error types for CLI exit-code routing.
//
// `main()` downcasts the top-level error chain and maps recognized markers to
// the documented exit codes. Unknown errors map to exit code 1.

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct DeviceNotFound(pub String);

impl fmt::Display for DeviceNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DeviceNotFound {}

#[derive(Debug)]
pub struct IoFailure(pub String);

impl fmt::Display for IoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for IoFailure {}

/// Walk the error chain and pick the first recognized exit code marker.
pub fn classify(err: &anyhow::Error) -> u8 {
    for cause in err.chain() {
        if cause.is::<DeviceNotFound>() {
            return 3;
        }
        if cause.is::<IoFailure>() {
            return 4;
        }
    }
    1
}

pub fn io(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(IoFailure(message.into()))
}

pub fn device_not_found(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(DeviceNotFound(message.into()))
}
