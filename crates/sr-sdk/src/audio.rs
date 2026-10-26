//! An Opus encoder and decoder for bots in voice (DEV-FU-002, D-377, D-378).
//!
//! The transport ([`crate::voice_transport`]) sends and receives whole 20 ms Opus packets and knows nothing of
//! PCM. A bot that has audio as samples (a synthesised tone, a decoded file, a microphone) encodes it here and
//! hands the packet to `send_opus(frame, level)`; a bot that wants to hear decodes the `VoiceEvent::Audio`
//! frames here. A bot that already holds Opus packets does not need this module at all.
//!
//! libopus 1.6.1 is built from the vendored source of `opusic-sys` by cmake. It lives only in the SDK: nothing
//! in `sr-voice`, `sr-api` or `sr-server` links it (D-201).

pub use opus::{Application, Channels};

/// The fixed sample rate of every frame, in hertz (the rate Opus voice uses on the wire).
pub const SAMPLE_RATE: u32 = 48_000;
/// Samples per channel in one 20 ms frame.
pub const FRAME_SAMPLES: usize = 960;
/// Room for the largest Opus packet one frame can produce (RFC 6716 caps a frame at 1275 bytes).
const MAX_PACKET: usize = 1500;
/// The longest packet libopus decodes (120 ms) at 48 kHz, per channel.
const MAX_DECODE_SAMPLES: usize = 5760;

/// What can go wrong encoding or decoding a frame.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// The PCM slice is not exactly one 20 ms frame (960 samples per channel, interleaved). It is never padded or
    /// cut silently.
    #[error("a frame is {expected} samples ({got} given): exactly 20 ms at 48 kHz")]
    FrameSize { expected: usize, got: usize },
    /// libopus refused the call.
    #[error("opus: {0}")]
    Opus(#[from] opus::Error),
}

fn channel_count(channels: Channels) -> usize {
    match channels {
        Channels::Mono => 1,
        Channels::Stereo => 2,
    }
}

/// Encodes 20 ms frames of 48 kHz 16-bit PCM into Opus packets.
pub struct OpusEncoder {
    inner: opus::Encoder,
    channels: usize,
}

impl OpusEncoder {
    /// A 48 kHz encoder. `Application::Voip` suits speech, `Application::Audio` music.
    pub fn new(channels: Channels, application: Application) -> Result<Self, AudioError> {
        Ok(Self {
            inner: opus::Encoder::new(SAMPLE_RATE, channels, application)?,
            channels: channel_count(channels),
        })
    }

    /// Samples (all channels, interleaved) that [`encode_frame`](Self::encode_frame) takes.
    pub fn frame_len(&self) -> usize {
        FRAME_SAMPLES * self.channels
    }

    /// Encode exactly one 20 ms frame: 960 samples per channel, interleaved for stereo. Any other length is
    /// [`AudioError::FrameSize`].
    pub fn encode_frame(&mut self, pcm: &[i16]) -> Result<Vec<u8>, AudioError> {
        let expected = self.frame_len();
        if pcm.len() != expected {
            return Err(AudioError::FrameSize {
                expected,
                got: pcm.len(),
            });
        }
        Ok(self.inner.encode_vec(pcm, MAX_PACKET)?)
    }

    /// Target bitrate in bits per second (libopus clamps it to its supported range).
    pub fn set_bitrate(&mut self, bits_per_second: i32) -> Result<(), AudioError> {
        Ok(self
            .inner
            .set_bitrate(opus::Bitrate::Bits(bits_per_second))?)
    }

    /// Put a low-bitrate copy of the previous frame into each packet so a receiver can repair one lost packet.
    pub fn set_inband_fec(&mut self, on: bool) -> Result<(), AudioError> {
        Ok(self.inner.set_inband_fec(on)?)
    }
}

/// Decodes Opus packets into 48 kHz 16-bit PCM.
pub struct OpusDecoder {
    inner: opus::Decoder,
    channels: usize,
}

impl OpusDecoder {
    /// A 48 kHz decoder.
    pub fn new(channels: Channels) -> Result<Self, AudioError> {
        Ok(Self {
            inner: opus::Decoder::new(SAMPLE_RATE, channels)?,
            channels: channel_count(channels),
        })
    }

    /// Decode one packet into interleaved samples. With `fec` the packet is the one that FOLLOWS a lost one and
    /// the result is the repaired lost frame (20 ms); an empty `packet` asks for loss concealment of one frame.
    /// A packet libopus rejects decodes to an empty buffer (a bot hearing garbage should hear silence, not die).
    pub fn decode_frame(&mut self, packet: &[u8], fec: bool) -> Vec<i16> {
        let per_channel = if fec || packet.is_empty() {
            FRAME_SAMPLES
        } else {
            MAX_DECODE_SAMPLES
        };
        let mut out = vec![0i16; per_channel * self.channels];
        match self.inner.decode(packet, &mut out, fec) {
            Ok(n) => {
                out.truncate(n * self.channels);
                out
            }
            Err(_) => Vec::new(),
        }
    }
}

/// The level of a frame in dBov for the RFC 6464 audio-level field of `send_opus(frame, level)`: 0 is full
/// scale, -127 is silence. Computed from the RMS of the samples, so a constant is no longer needed.
pub fn audio_level_dbov(pcm: &[i16]) -> i8 {
    if pcm.is_empty() {
        return -127;
    }
    let sum: f64 = pcm.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    let rms = (sum / pcm.len() as f64).sqrt() / 32768.0;
    if rms <= 0.0 {
        return -127;
    }
    (20.0 * rms.log10()).round().clamp(-127.0, 0.0) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, channels: usize) -> Vec<i16> {
        (0..frames * FRAME_SAMPLES)
            .flat_map(|i| {
                let t = i as f64 / 48_000.0;
                let v = (f64::sin(2.0 * std::f64::consts::PI * 440.0 * t) * 12_000.0) as i16;
                std::iter::repeat_n(v, channels)
            })
            .collect()
    }

    /// Normalised correlation of two buffers at the best lag up to `max_lag` (the codec delays the signal).
    fn best_correlation(a: &[i16], b: &[i16], max_lag: usize) -> f64 {
        let mut best = f64::MIN;
        for lag in 0..=max_lag {
            let n = a.len().min(b.len() - lag);
            let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
            for i in 0..n {
                let (x, y) = (f64::from(a[i]), f64::from(b[i + lag]));
                ab += x * y;
                aa += x * x;
                bb += y * y;
            }
            if aa > 0.0 && bb > 0.0 {
                best = best.max(ab / (aa.sqrt() * bb.sqrt()));
            }
        }
        best
    }

    #[test]
    fn a_tone_survives_the_round_trip() {
        let mut enc = OpusEncoder::new(Channels::Mono, Application::Audio).unwrap();
        let mut dec = OpusDecoder::new(Channels::Mono).unwrap();
        let pcm = tone(10, 1);
        let mut out = Vec::new();
        for frame in pcm.chunks(FRAME_SAMPLES) {
            let packet = enc.encode_frame(frame).unwrap();
            assert!(!packet.is_empty() && packet.len() < 400, "{}", packet.len());
            let back = dec.decode_frame(&packet, false);
            assert_eq!(back.len(), FRAME_SAMPLES);
            out.extend(back);
        }
        // Skip the first frames (encoder warm-up) and allow for the codec delay.
        let c = best_correlation(&pcm[3 * FRAME_SAMPLES..], &out[3 * FRAME_SAMPLES..], 480);
        assert!(c > 0.9, "correlation {c}");
        let level = audio_level_dbov(&out[3 * FRAME_SAMPLES..]);
        assert!((-14..=-6).contains(&level), "level {level}");
    }

    #[test]
    fn stereo_frames_are_interleaved() {
        let mut enc = OpusEncoder::new(Channels::Stereo, Application::Voip).unwrap();
        let mut dec = OpusDecoder::new(Channels::Stereo).unwrap();
        assert_eq!(enc.frame_len(), 1920);
        let packet = enc.encode_frame(&tone(1, 2)).unwrap();
        assert_eq!(dec.decode_frame(&packet, false).len(), 1920);
    }

    #[test]
    fn a_ten_millisecond_slice_is_refused() {
        let mut enc = OpusEncoder::new(Channels::Mono, Application::Voip).unwrap();
        let err = enc.encode_frame(&[0i16; 480]).unwrap_err();
        assert!(matches!(
            err,
            AudioError::FrameSize {
                expected: 960,
                got: 480
            }
        ));
        assert!(enc.encode_frame(&[]).is_err());
        // Stereo wants 1920 samples, not 960.
        let mut st = OpusEncoder::new(Channels::Stereo, Application::Voip).unwrap();
        assert!(st.encode_frame(&[0i16; 960]).is_err());
    }

    #[test]
    fn settings_and_garbage_are_handled() {
        let mut enc = OpusEncoder::new(Channels::Mono, Application::Voip).unwrap();
        enc.set_bitrate(24_000).unwrap();
        enc.set_inband_fec(true).unwrap();
        let mut dec = OpusDecoder::new(Channels::Mono).unwrap();
        assert!(dec.decode_frame(&[0xFF; 3], false).is_empty());
        // Loss concealment of one frame.
        assert_eq!(dec.decode_frame(&[], false).len(), FRAME_SAMPLES);
    }

    #[test]
    fn levels_follow_the_signal() {
        assert_eq!(audio_level_dbov(&[]), -127);
        assert_eq!(audio_level_dbov(&[0; 960]), -127);
        assert_eq!(audio_level_dbov(&[i16::MAX; 960]), 0);
        // Half scale is about -6 dBov.
        assert_eq!(audio_level_dbov(&[16384; 960]), -6);
    }
}
