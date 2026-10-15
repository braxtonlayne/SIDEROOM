//! Simulcast layer selection per subscriber (VOX-SFU-005).
//!
//! A publisher's camera arrives as up to three layers (`q` quarter, `h` half, `f` full
//! resolution). Each subscriber gets exactly one of them per source. [`LayerSelector`]
//! decides which, from four inputs:
//!
//! * the **hint** the client sent: an explicit layer or the size of the tile the video is
//!   drawn in ([`layer_for_viewport`]);
//! * the **bandwidth estimate** for the subscriber (TWCC, `EgressBitrateEstimate`),
//!   shared between its sources;
//! * the **governor cap** ([`crate::governor`]);
//! * which layers the publisher is **actually sending**, and at what bitrate.
//!
//! # Behaviour
//!
//! * Down is immediate: a lower cap, a lower estimate or a vanished layer switches at the
//!   next opportunity.
//! * Up waits [`SelectorConfig::upgrade_hold`] of continuous headroom, so a briefly good
//!   estimate does not make the picture flip-flop.
//! * A switch only takes effect on a **keyframe of the new layer** ([`LayerSelector::on_frame`]),
//!   so the decoder never sees a stream that starts mid-GOP. Until then the old layer
//!   keeps flowing, and the caller asks the publisher for a keyframe
//!   ([`crate::keyframe`]).
//!
//! The selector holds no clock: callers pass `now`.

use std::time::{Duration, Instant};

use sr_voice_proto::video::{Layer, Viewport};

/// Tuning knobs. The defaults suit a 720p camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectorConfig {
    /// How long a better layer must keep fitting before the subscriber is moved up.
    pub upgrade_hold: Duration,
    /// Use at most this fraction of the bandwidth estimate for video.
    pub headroom: f64,
    /// Assumed bitrate per layer (bits/s) until the real rate has been measured.
    pub default_bps: [f64; 3],
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            upgrade_hold: Duration::from_secs(4),
            headroom: 0.85,
            default_bps: [150_000.0, 500_000.0, 1_500_000.0],
        }
    }
}

/// The smallest layer whose picture is at least as tall as `viewport`, given the full
/// layer's height (`full_height`, 0 = unknown, taken as 720). The layers are 1/4, 1/2 and
/// full height.
pub fn layer_for_viewport(viewport: Viewport, full_height: u32) -> Layer {
    let full = if full_height == 0 { 720 } else { full_height };
    let want = viewport.height;
    if want <= full / 4 {
        Layer::Low
    } else if want <= full / 2 {
        Layer::Mid
    } else {
        Layer::High
    }
}

/// What the publisher is sending right now.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Available {
    /// Bits per second measured per layer, `None` when the layer is not being sent.
    pub bps: [Option<f64>; 3],
}

impl Available {
    pub fn has(&self, l: Layer) -> bool {
        self.bps[l.index()].is_some()
    }

    pub fn any(&self) -> bool {
        self.bps.iter().any(Option::is_some)
    }

    /// Layers the publisher sends, smallest first.
    pub fn layers(&self) -> impl Iterator<Item = Layer> + '_ {
        Layer::ALL.into_iter().filter(|l| self.has(*l))
    }

    /// A single-layer publisher (a screen share) is sent as its only layer.
    pub fn only(&self) -> Option<Layer> {
        let mut it = self.layers();
        let first = it.next()?;
        it.next().is_none().then_some(first)
    }
}

/// What a frame of a layer should do for this subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// Forward it.
    Forward,
    /// Drop it (another layer is current).
    Drop,
    /// Forward it, and the subscriber just moved to this layer.
    Switched(Layer),
}

/// One subscriber's view of one source.
#[derive(Debug, Clone)]
pub struct LayerSelector {
    cfg: SelectorConfig,
    /// The layer being forwarded; `None` until the first keyframe arrives.
    current: Option<Layer>,
    /// The layer to move to at its next keyframe.
    pending: Option<Layer>,
    /// The client's own limit (explicit layer or viewport), if any.
    hint_cap: Layer,
    gov_cap: Layer,
    /// When a better layer started to fit; reset when it stops fitting.
    upgrade_since: Option<Instant>,
}

impl LayerSelector {
    pub fn new(cfg: SelectorConfig) -> Self {
        Self {
            cfg,
            current: None,
            pending: None,
            // A thumbnail until the client says how big the tile is.
            hint_cap: Layer::Low,
            gov_cap: Layer::High,
            upgrade_since: None,
        }
    }

    pub fn current(&self) -> Option<Layer> {
        self.current
    }

    pub fn pending(&self) -> Option<Layer> {
        self.pending
    }

    /// The layer to ask the publisher for a keyframe on, if a switch is waiting.
    pub fn wants_keyframe_on(&self) -> Option<Layer> {
        self.pending
    }

    pub fn hint_cap(&self) -> Layer {
        self.hint_cap
    }

    pub fn set_hint_cap(&mut self, cap: Layer) {
        self.hint_cap = cap;
    }

    pub fn set_governor_cap(&mut self, cap: Layer) {
        self.gov_cap = cap;
    }

    pub fn governor_cap(&self) -> Layer {
        self.gov_cap
    }

    /// The best layer for the inputs: the largest available layer within both caps whose
    /// bitrate fits `budget_bps` (`None` = no estimate yet, which does not limit).
    pub fn ideal(&self, avail: &Available, budget_bps: Option<f64>) -> Option<Layer> {
        if let Some(only) = avail.only() {
            // One layer is not a choice (a screen share, or a camera without simulcast).
            return Some(only);
        }
        let cap = self.hint_cap.min(self.gov_cap);
        let fits = |l: Layer| {
            let Some(budget) = budget_bps else {
                return true;
            };
            let rate = avail.bps[l.index()]
                .filter(|r| *r > 0.0)
                .unwrap_or(self.cfg.default_bps[l.index()]);
            rate <= budget * self.cfg.headroom
        };
        let mut best = None;
        for l in avail.layers() {
            if l <= cap && fits(l) {
                best = Some(l);
            }
        }
        // Nothing fits: the smallest layer there is is better than no picture.
        best.or_else(|| avail.layers().next())
    }

    /// Re-evaluate. Returns the layer to request a keyframe on when a switch is now pending.
    pub fn update(
        &mut self,
        now: Instant,
        avail: &Available,
        budget_bps: Option<f64>,
    ) -> Option<Layer> {
        let Some(ideal) = self.ideal(avail, budget_bps) else {
            // The publisher stopped: nothing to forward or wait for.
            self.current = None;
            self.pending = None;
            self.upgrade_since = None;
            return None;
        };
        // The layer in use vanished: move at once, whatever the hold says.
        if let Some(cur) = self.current
            && !avail.has(cur)
        {
            self.current = None;
        }
        let Some(cur) = self.current else {
            // Not forwarding yet: wait for the first keyframe of the ideal layer.
            let changed = self.pending != Some(ideal);
            self.pending = Some(ideal);
            self.upgrade_since = None;
            return changed.then_some(ideal);
        };
        if ideal == cur {
            self.pending = None;
            self.upgrade_since = None;
            return None;
        }
        if ideal < cur {
            // Down is immediate.
            self.upgrade_since = None;
            let changed = self.pending != Some(ideal);
            self.pending = Some(ideal);
            return changed.then_some(ideal);
        }
        // Up: only after the hold.
        let since = *self.upgrade_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= self.cfg.upgrade_hold {
            let changed = self.pending != Some(ideal);
            self.pending = Some(ideal);
            return changed.then_some(ideal);
        }
        // Still holding: a pending downgrade from before is cancelled by a better outlook.
        if self.pending.is_some_and(|p| p < cur) {
            self.pending = None;
        }
        None
    }

    /// A frame of `layer` arrived. Decides whether this subscriber gets it.
    pub fn on_frame(&mut self, layer: Layer, keyframe: bool) -> Frame {
        if let Some(p) = self.pending
            && p == layer
            && keyframe
        {
            self.current = Some(p);
            self.pending = None;
            self.upgrade_since = None;
            return Frame::Switched(p);
        }
        if self.current == Some(layer) {
            Frame::Forward
        } else {
            Frame::Drop
        }
    }

    /// A single-layer source (no simulcast): every frame is forwarded.
    pub fn force_single(&mut self, layer: Layer) {
        self.current = Some(layer);
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(low: f64, mid: f64, high: f64) -> Available {
        Available {
            bps: [Some(low), Some(mid), Some(high)],
        }
    }

    fn sel() -> (LayerSelector, Instant) {
        (
            LayerSelector::new(SelectorConfig::default()),
            Instant::now(),
        )
    }

    #[test]
    fn viewports_pick_the_smallest_layer_that_covers_them() {
        let vp = |h| Viewport {
            width: h * 16 / 9,
            height: h,
        };
        assert_eq!(layer_for_viewport(vp(120), 720), Layer::Low);
        assert_eq!(layer_for_viewport(vp(180), 720), Layer::Low);
        assert_eq!(layer_for_viewport(vp(181), 720), Layer::Mid);
        assert_eq!(layer_for_viewport(vp(360), 720), Layer::Mid);
        assert_eq!(layer_for_viewport(vp(361), 720), Layer::High);
        assert_eq!(layer_for_viewport(vp(1080), 720), Layer::High);
        // Unknown publisher height is taken as 720p.
        assert_eq!(layer_for_viewport(vp(200), 0), Layer::Mid);
        // A 1080p source has bigger layers.
        assert_eq!(layer_for_viewport(vp(270), 1080), Layer::Low);
        assert_eq!(layer_for_viewport(vp(540), 1080), Layer::Mid);
    }

    #[test]
    fn a_new_subscriber_starts_with_the_thumbnail() {
        let (mut s, t) = sel();
        let a = all(150e3, 500e3, 1500e3);
        assert_eq!(s.update(t, &a, None), Some(Layer::Low));
        // Nothing flows before a keyframe.
        assert_eq!(s.on_frame(Layer::Low, false), Frame::Drop);
        assert_eq!(s.on_frame(Layer::Low, true), Frame::Switched(Layer::Low));
        assert_eq!(s.on_frame(Layer::Low, false), Frame::Forward);
        // Other layers are never forwarded.
        assert_eq!(s.on_frame(Layer::High, true), Frame::Drop);
        assert_eq!(s.current(), Some(Layer::Low));
    }

    #[test]
    fn the_hint_raises_the_ceiling_but_upgrades_wait_for_the_hold() {
        let (mut s, t0) = sel();
        let a = all(150e3, 500e3, 1500e3);
        s.update(t0, &a, None);
        s.on_frame(Layer::Low, true);
        s.set_hint_cap(Layer::High);
        // Better outlook, but not yet for long enough.
        assert_eq!(s.update(t0 + Duration::from_secs(1), &a, None), None);
        assert_eq!(s.update(t0 + Duration::from_secs(3), &a, None), None);
        assert_eq!(s.pending(), None);
        // After the hold the switch is pending and the old layer keeps flowing.
        assert_eq!(
            s.update(t0 + Duration::from_secs(5), &a, None),
            Some(Layer::High)
        );
        assert_eq!(s.wants_keyframe_on(), Some(Layer::High));
        assert_eq!(s.on_frame(Layer::Low, false), Frame::Forward);
        // A delta frame of the new layer does not switch; its keyframe does.
        assert_eq!(s.on_frame(Layer::High, false), Frame::Drop);
        assert_eq!(s.on_frame(Layer::High, true), Frame::Switched(Layer::High));
        assert_eq!(s.on_frame(Layer::Low, false), Frame::Drop);
        assert_eq!(s.current(), Some(Layer::High));
        assert_eq!(s.pending(), None);
    }

    #[test]
    fn downgrades_are_immediate() {
        let (mut s, t0) = sel();
        let a = all(150e3, 500e3, 1500e3);
        s.set_hint_cap(Layer::High);
        s.update(t0, &a, None);
        s.on_frame(Layer::High, true);
        assert_eq!(s.current(), Some(Layer::High));
        // The estimate collapses to 400 kbit/s: only the thumbnail fits.
        assert_eq!(s.update(t0, &a, Some(400e3)), Some(Layer::Low));
        assert_eq!(s.on_frame(Layer::High, false), Frame::Forward);
        assert_eq!(s.on_frame(Layer::Low, true), Frame::Switched(Layer::Low));
        // The governor lowering its cap does the same.
        let (mut g, t) = sel();
        g.set_hint_cap(Layer::High);
        g.update(t, &a, None);
        g.on_frame(Layer::High, true);
        g.set_governor_cap(Layer::Mid);
        assert_eq!(g.update(t, &a, None), Some(Layer::Mid));
    }

    #[test]
    fn the_estimate_is_shared_with_headroom() {
        let (mut s, t) = sel();
        s.set_hint_cap(Layer::High);
        let a = all(150e3, 500e3, 1500e3);
        // 85% of 1.6 Mbit/s = 1.36: the full layer (1.5) does not fit, the half layer does.
        assert_eq!(s.ideal(&a, Some(1_600e3)), Some(Layer::Mid));
        assert_eq!(s.ideal(&a, Some(1_800e3)), Some(Layer::High));
        // No estimate yet does not limit.
        assert_eq!(s.ideal(&a, None), Some(Layer::High));
        // Below even the thumbnail still sends the thumbnail.
        assert_eq!(s.ideal(&a, Some(10e3)), Some(Layer::Low));
        let _ = t;
    }

    #[test]
    fn measured_rates_beat_the_defaults() {
        let (mut s, _) = sel();
        s.set_hint_cap(Layer::High);
        // The full layer is only 300 kbit/s (a static scene): it fits a small budget.
        let a = all(50e3, 120e3, 300e3);
        assert_eq!(s.ideal(&a, Some(400e3)), Some(Layer::High));
    }

    #[test]
    fn a_vanished_layer_is_replaced_at_once() {
        let (mut s, t) = sel();
        s.set_hint_cap(Layer::High);
        let a = all(150e3, 500e3, 1500e3);
        s.update(t, &a, None);
        s.on_frame(Layer::High, true);
        // The publisher drops the full layer (its own bandwidth fell).
        let b = Available {
            bps: [Some(150e3), Some(500e3), None],
        };
        assert_eq!(s.update(t, &b, None), Some(Layer::Mid));
        assert_eq!(
            s.current(),
            None,
            "nothing flows until a keyframe of the new layer"
        );
        assert_eq!(s.on_frame(Layer::Mid, true), Frame::Switched(Layer::Mid));
    }

    #[test]
    fn single_layer_sources_are_not_a_choice() {
        let (mut s, t) = sel();
        // A screen share arrives on one layer (rid-less streams are reported as High).
        let one = Available {
            bps: [None, None, Some(2e6)],
        };
        // Even a tiny budget and a thumbnail hint keep the only layer.
        assert_eq!(s.update(t, &one, Some(50e3)), Some(Layer::High));
        assert_eq!(s.on_frame(Layer::High, true), Frame::Switched(Layer::High));
        assert_eq!(s.on_frame(Layer::High, false), Frame::Forward);
    }

    #[test]
    fn nothing_available_means_nothing_forwarded() {
        let (mut s, t) = sel();
        let a = all(150e3, 500e3, 1500e3);
        s.update(t, &a, None);
        s.on_frame(Layer::Low, true);
        assert_eq!(s.update(t, &Available::default(), None), None);
        assert_eq!(s.current(), None);
        assert_eq!(s.pending(), None);
        assert_eq!(s.on_frame(Layer::Low, true), Frame::Drop);
    }

    #[test]
    fn a_better_outlook_cancels_a_pending_downgrade() {
        let (mut s, t0) = sel();
        s.set_hint_cap(Layer::High);
        let a = all(150e3, 500e3, 1500e3);
        s.update(t0, &a, None);
        s.on_frame(Layer::High, true);
        // A dip asks for Mid; before its keyframe arrives the estimate recovers.
        assert_eq!(s.update(t0, &a, Some(700e3)), Some(Layer::Mid));
        assert_eq!(s.update(t0 + Duration::from_millis(100), &a, None), None);
        assert_eq!(s.pending(), None, "back on the current layer");
        assert_eq!(s.on_frame(Layer::High, false), Frame::Forward);
    }

    #[test]
    fn upgrades_that_keep_failing_do_not_accumulate() {
        let (mut s, t0) = sel();
        s.set_hint_cap(Layer::High);
        let a = all(150e3, 500e3, 1500e3);
        s.update(t0, &a, Some(400e3));
        s.on_frame(Layer::Low, true);
        // The estimate flaps: headroom for 3 s, none for a moment, headroom again.
        s.update(t0 + Duration::from_secs(1), &a, Some(2_000e3));
        s.update(t0 + Duration::from_secs(3), &a, Some(2_000e3));
        s.update(t0 + Duration::from_secs(3), &a, Some(300e3));
        // The hold restarts: 4 s after the second good reading, not the first.
        assert_eq!(
            s.update(t0 + Duration::from_secs(5), &a, Some(2_000e3)),
            None
        );
        assert_eq!(
            s.update(t0 + Duration::from_secs(8), &a, Some(2_000e3)),
            None
        );
        assert_eq!(
            s.update(t0 + Duration::from_secs(9), &a, Some(2_000e3)),
            Some(Layer::High)
        );
    }
}
