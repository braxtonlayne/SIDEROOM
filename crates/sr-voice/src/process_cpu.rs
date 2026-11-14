//! `process_cpu_seconds_total` (D-706, VOX-SFU-009): the CPU time of this process, user plus system, the usual
//! Prometheus name. On Linux it is read from `/proc/self/stat` every 15 seconds and set through the `metrics`
//! facade as an absolute counter value. Other systems compile this module and report nothing (the metric is never
//! described). It lives here because both the standalone SFU and `sr-server` already depend on this crate; with the
//! SFU inside `sr-server` the figure is the whole server's.
//!
//! The counter is whole seconds (the facade's counters are integers), refreshed every 15 seconds.

/// The metric's name.
pub const PROCESS_CPU: &str = "process_cpu_seconds_total";

/// Clock ticks per second in `/proc/<pid>/stat` (USER_HZ is 100 on every Linux the project supports).
pub const TICKS_PER_SECOND: u64 = 100;

/// `utime + stime` in clock ticks, from the text of `/proc/self/stat`. The command name (field 2) sits in
/// parentheses and may hold spaces and brackets, so the fields are counted from after the last `)`: the rest starts
/// with field 3 (the state), which makes `utime` and `stime` (fields 14 and 15 of the whole line) the 12th and 13th
/// of the rest. `None` for a line that is too short or not a number.
pub fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_ascii_whitespace().skip(11);
    let utime: u64 = fields.next()?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    utime.checked_add(stime)
}

/// The counter only goes up: a reading below the last one (a clock that went back, a bad parse) leaves the last.
pub fn raise(last: &mut u64, reading: u64) -> u64 {
    if reading > *last {
        *last = reading;
    }
    *last
}

/// Describes the metric and starts the one task that refreshes it. Safe to call more than once (the SFU's start and
/// the server's both do): only the first call acts. Needs a Tokio runtime; without one it does nothing. A no-op on
/// systems other than Linux.
pub fn start() {
    #[cfg(target_os = "linux")]
    linux::start();
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{PROCESS_CPU, TICKS_PER_SECOND, parse_cpu_ticks, raise};
    use std::{sync::Once, time::Duration};

    const REFRESH: Duration = Duration::from_secs(15);

    pub(super) fn start() {
        static STARTED: Once = Once::new();
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            return;
        };
        STARTED.call_once(|| {
            metrics::describe_counter!(
                PROCESS_CPU,
                "Total user and system CPU time of this process in seconds"
            );
            rt.spawn(async {
                let counter = metrics::counter!(PROCESS_CPU);
                let mut last = 0u64;
                let mut tick = tokio::time::interval(REFRESH);
                loop {
                    tick.tick().await;
                    if let Some(ticks) = std::fs::read_to_string("/proc/self/stat")
                        .ok()
                        .as_deref()
                        .and_then(parse_cpu_ticks)
                    {
                        counter.absolute(raise(&mut last, ticks) / TICKS_PER_SECOND);
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real lines have 52 fields; only the first 15 matter here. utime = 14th, stime = 15th.
    const PLAIN: &str = "4242 (sr-server) S 1 4242 4242 0 -1 4194560 1000 0 0 0 1234 567 0 0 20 0 9 0 100 1000000 500 18446744073709551615";
    const SPACES_AND_BRACKETS: &str = "17 (tokio) (rt) w 1) S 1 17 17 0 -1 4194560 10 0 0 0 300 20 0 0 20 0 1 0 5 100 50 18446744073709551615";
    const ZERO: &str = "1 (init) S 0 1 1 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 1 1 1 1";

    #[test]
    fn the_parser_adds_user_and_system_time() {
        assert_eq!(parse_cpu_ticks(PLAIN), Some(1234 + 567));
        assert_eq!(parse_cpu_ticks(ZERO), Some(0));
        assert_eq!(parse_cpu_ticks(&format!("{PLAIN}\n")), Some(1801));
    }

    #[test]
    fn a_command_name_with_brackets_and_spaces_does_not_shift_the_fields() {
        assert_eq!(parse_cpu_ticks(SPACES_AND_BRACKETS), Some(320));
    }

    #[test]
    fn a_short_line_or_a_non_number_gives_nothing() {
        assert_eq!(parse_cpu_ticks(""), None);
        assert_eq!(parse_cpu_ticks("1 (x) S 1 2 3"), None);
        assert_eq!(parse_cpu_ticks("no brackets at all"), None);
        // utime is not a number.
        let bad = "1 (x) S 0 1 1 0 -1 4194560 0 0 0 0 abc 5 0 0 20";
        assert_eq!(parse_cpu_ticks(bad), None);
        // stime is missing.
        let short = "1 (x) S 0 1 1 0 -1 4194560 0 0 0 0 7";
        assert_eq!(parse_cpu_ticks(short), None);
    }

    #[test]
    fn the_cpu_counter_never_goes_down() {
        let mut last = 0;
        assert_eq!(raise(&mut last, parse_cpu_ticks(PLAIN).unwrap()), 1801);
        // A later reading that is lower (fed in descending order) leaves the larger.
        assert_eq!(
            raise(&mut last, parse_cpu_ticks(SPACES_AND_BRACKETS).unwrap()),
            1801
        );
        assert_eq!(last, 1801);
        assert_eq!(raise(&mut last, 2000), 2000);
    }

    // SEC-AUD-235: the module's own parser on the real file. The wave 68 rule "no test depends on /proc" is for tests
    // that run on every system; this one only exists on Linux.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_proc_stat_gives_a_value_that_does_not_go_back() {
        let read = || {
            let stat = std::fs::read_to_string("/proc/self/stat").expect("/proc/self/stat");
            parse_cpu_ticks(&stat).expect("utime and stime in /proc/self/stat")
        };
        let first = read();
        let start = std::time::Instant::now();
        let mut x = 0u64;
        while start.elapsed() < std::time::Duration::from_millis(50) {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        assert!(
            read() >= first,
            "CPU ticks went back after a busy loop ({x})"
        );
    }
}
