//! Utilities for timestamping observable emissions.
//!
//! Provides the `TimestampedEmit` type for pairing values emitted by an `Observable`
//! with their corresponding timestamps.

use std::{
    fmt::Display,
    time::{Duration, SystemTime, SystemTimeError},
};

/// Represents a timestamped object created from an observable stream.
///
/// This type is emitted by the `Observable` created by the [`timestamp()`] operator,
/// which maps each value emitted by the source observable to a corresponding
/// timestamp. The `value` property holds the emitted value along with its type,
/// while the `timestamp` is generated using `SystemTime::now()`, representing the
/// duration since the `UNIX_EPOCH` in milliseconds.
///
/// [`timestamp()`]: ../trait.ObservableExt.html#method.timestamp
#[derive(Clone, Debug)]
pub struct TimestampedEmit<T> {
    pub value: T,
    pub timestamp: u128,
    error: Option<&'static str>,
    sys_time_error: Option<SystemTimeError>,
}

impl<T> TimestampedEmit<T> {
    /// If `SystemTimeError` occurred during timestamping emitted values, get
    /// duration which represents how far forward the time of the emit was from the
    /// `UNIX_EPOCH`, otherwise return `None`.
    pub fn error_duration(&self) -> Option<Duration> {
        if let Some(ste) = &self.sys_time_error {
            return Some(ste.duration());
        }
        None
    }

    /// Returns an error string if the system time moved backwards, such as from a
    /// misconfigured system clock; otherwise, returns `None`.
    pub fn error_str(&self) -> Option<&'static str> {
        self.error
    }

    /// A convenience method to check if the emitted value is mapped with a proper timestamp.
    pub fn is_error(&self) -> bool {
        if self.error.is_none() {
            return false;
        }
        true
    }

    pub(super) fn new(value: T) -> TimestampedEmit<T> {
        let mut error = None;
        let mut sys_time_error = None;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|ste| {
                error = Some("Error: Attempted to calculate a time before the Unix epoch. Your system clock might be misconfigured.");
                sys_time_error = Some(ste);

                // Using fallback timestamp of 0.
                Duration::new(0, 0)
            })
            .as_millis();

        TimestampedEmit {
            value,
            timestamp,
            error,
            sys_time_error,
        }
    }
}

impl<T: Display> Display for TimestampedEmit<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "value: {}\ntimestamp: {}", self.value, self.timestamp)
    }
}
