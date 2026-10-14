//! Parser for `ffmpeg -progress pipe:1` (`key=value` blocks closed by `progress=`).

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProgressSample {
    /// Position in the output, seconds.
    pub out_time_secs: Option<f64>,
    pub fps: Option<f32>,
    /// Encode speed as a multiple of real time.
    pub speed: Option<f32>,
    pub frame: Option<u64>,
    /// The block said `progress=end`.
    pub done: bool,
}

#[derive(Debug, Default)]
pub struct ProgressParser {
    cur: ProgressSample,
}

/// `HH:MM:SS.micro` to seconds.
fn parse_clock(s: &str) -> Option<f64> {
    let mut it = s.trim().split(':');
    let (h, m, sec) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let v = h.parse::<f64>().ok()? * 3600.0
        + m.parse::<f64>().ok()? * 60.0
        + sec.parse::<f64>().ok()?;
    (v.is_finite() && v >= 0.0).then_some(v)
}

impl ProgressParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one stdout line. Returns a sample when a `progress=` line closes a block.
    pub fn feed(&mut self, line: &str) -> Option<ProgressSample> {
        let (key, value) = line.trim().split_once('=')?;
        let value = value.trim();
        match key {
            // `out_time_ms` is microseconds despite its name (a long-standing ffmpeg quirk).
            "out_time_us" | "out_time_ms" => {
                if let Ok(us) = value.parse::<i64>()
                    && us >= 0
                {
                    self.cur.out_time_secs = Some(us as f64 / 1_000_000.0);
                }
            }
            "out_time" => {
                if self.cur.out_time_secs.is_none() {
                    self.cur.out_time_secs = parse_clock(value);
                }
            }
            "fps" => self.cur.fps = value.parse().ok().filter(|f: &f32| f.is_finite()),
            "frame" => self.cur.frame = value.parse().ok(),
            "speed" => {
                self.cur.speed = value
                    .trim_end_matches('x')
                    .trim()
                    .parse()
                    .ok()
                    .filter(|f: &f32| f.is_finite() && *f >= 0.0);
            }
            "progress" => {
                self.cur.done = value == "end";
                return Some(std::mem::take(&mut self.cur));
            }
            _ => {}
        }
        None
    }
}

/// Overall job percent from finished step weights and the running step.
pub fn overall_percent(
    done_weight: f64,
    total_weight: f64,
    step_weight: f64,
    out_time: Option<f64>,
    expect_secs: f64,
) -> f32 {
    if total_weight <= 0.0 {
        return 0.0;
    }
    let frac = match out_time {
        Some(t) if expect_secs > 0.0 => (t / expect_secs).clamp(0.0, 1.0),
        _ => 0.0,
    };
    (((done_weight + step_weight * frac) / total_weight) * 100.0).clamp(0.0, 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCKS: &str = "frame=240\nfps=48.00\nstream_0_0_q=-1.0\nbitrate=N/A\ntotal_size=N/A\n\
        out_time_us=10010000\nout_time_ms=10010000\nout_time=00:00:10.010000\ndup_frames=0\n\
        drop_frames=0\nspeed=2.01x\nprogress=continue\n\
        frame=480\nfps=51.5\nout_time_us=N/A\nout_time=00:00:20.020000\nspeed=N/A\nprogress=end\n";

    #[test]
    fn parses_blocks() {
        let mut p = ProgressParser::new();
        let samples: Vec<_> = BLOCKS.lines().filter_map(|l| p.feed(l)).collect();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].frame, Some(240));
        assert_eq!(samples[0].fps, Some(48.0));
        assert_eq!(samples[0].speed, Some(2.01));
        assert!((samples[0].out_time_secs.unwrap() - 10.01).abs() < 1e-9);
        assert!(!samples[0].done);
        // N/A microseconds fall back to the clock string.
        assert!((samples[1].out_time_secs.unwrap() - 20.02).abs() < 1e-9);
        assert_eq!(samples[1].speed, None);
        assert!(samples[1].done);
    }

    #[test]
    fn ignores_garbage_and_negative_times() {
        let mut p = ProgressParser::new();
        assert_eq!(p.feed("not a progress line"), None);
        assert_eq!(p.feed("out_time_us=-9223372036854775807"), None);
        let s = p.feed("progress=continue").unwrap();
        assert_eq!(s.out_time_secs, None);
        assert_eq!(s, ProgressSample::default());
    }

    #[test]
    fn percent() {
        assert_eq!(overall_percent(0.0, 1.0, 0.8, Some(50.0), 100.0), 40.0);
        assert_eq!(overall_percent(0.8, 1.0, 0.1, Some(100.0), 100.0), 90.0);
        assert_eq!(overall_percent(0.0, 1.0, 0.8, Some(500.0), 100.0), 80.0);
        assert_eq!(overall_percent(0.0, 1.0, 0.8, None, 100.0), 0.0);
        assert_eq!(overall_percent(0.0, 0.0, 0.0, None, 0.0), 0.0);
    }
}
