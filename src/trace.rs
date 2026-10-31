//! Opt-in phase timings for diagnosing native build costs.

use std::ffi::OsStr;
use std::fmt::Arguments;
use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn event(start: Instant, detail: Arguments<'_>) {
    if *ENABLED.get_or_init(|| std::env::var_os("BIFOIL_TRACE_TIMINGS").as_deref() == Some(OsStr::new("1"))) {
        eprintln!("bifoilc timing {:.3}s: {detail}", start.elapsed().as_secs_f64());
    }
}
