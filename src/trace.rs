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

#[cfg(test)]
mod tests {
    use super::event;
    use std::process::Command;
    use std::time::Instant;

    #[test]
    fn emit_timing() {
        event(Instant::now(), format_args!("original phase marker"));
    }

    #[test]
    fn timing_is_opt_in_and_does_not_change_the_phase_message() {
        for enabled in [false, true] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "trace::tests::emit_timing", "--nocapture"]);
            command.env_remove("BIFOIL_TRACE_TIMINGS");
            if enabled {
                command.env("BIFOIL_TRACE_TIMINGS", "1");
            }
            let output = command.output().unwrap();
            assert!(output.status.success());
            let errors = String::from_utf8(output.stderr).unwrap();
            assert_eq!(errors.contains("bifoilc timing"), enabled);
            assert_eq!(errors.contains("original phase marker"), enabled);
        }
    }
}
