use std::{cmp::Ordering, fmt};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TimeBase {
    pub numerator: i32,
    pub denominator: i32,
}

impl TimeBase {
    pub fn seconds(self, ticks: i64) -> f64 {
        ticks as f64 * f64::from(self.numerator) / f64::from(self.denominator)
    }

    pub(crate) fn parse(text: &str) -> Result<Self> {
        let (numerator, denominator) = text.split_once('/').context("invalid time base")?;
        let numerator = numerator.parse()?;
        let denominator = denominator.parse()?;
        ensure!(numerator > 0 && denominator > 0, "invalid time base {text}");
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub(crate) fn compare(self, ticks: i128, other: Self, other_ticks: i128) -> Ordering {
        (ticks * i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(other_ticks * i128::from(other.numerator) * i128::from(self.denominator)))
    }

    pub(crate) fn micros(self, ticks: i128) -> Result<i64> {
        let scaled = ticks * i128::from(self.numerator) * 1_000_000;
        let denominator = i128::from(self.denominator);
        let rounded = (scaled.abs() + denominator / 2) / denominator * scaled.signum();
        i64::try_from(rounded).context("timestamp outside FFmpeg's microsecond range")
    }
}

impl fmt::Display for TimeBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

pub(crate) fn ffmpeg_time(micros: i64) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let abs = micros.unsigned_abs();
    format!("{sign}{}.{:06}", abs / 1_000_000, abs % 1_000_000)
}
