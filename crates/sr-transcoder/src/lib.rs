//! sr-transcoder: the GPU/CPU transcode worker (CIN-ING-005).
//!
//! * [`encoders`]: detect `libx264` / `h264_nvenc`, zscale/tonemap and friends by parsing
//!   ffmpeg's own listings.
//! * [`args`]: exact ffmpeg argument vectors for every step of a job.
//! * [`progress`]: `-progress pipe:1` parsing.
//! * [`worker`]: the job state machine and loop against the `WorkerApi`, `Transfer` and
//!   `CommandRunner` ports.
//! * [`cli`]: command-line parsing.
//!
//! The default build runs the `ffmpeg` command-line tool; the `native-ffmpeg` feature (off by
//! default, reserved) is where in-process `ffmpeg-next` use would live.

pub mod args;
pub mod cli;
pub mod encoders;
pub mod http_client;
pub mod progress;
pub mod slate;
pub mod worker;
