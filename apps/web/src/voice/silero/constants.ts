/** D-1050: the numbers the worklet, the worker and the client agree on (Silero VAD v5 at 16 kHz). */
export const SAMPLE_RATE = 16_000;
/** Samples the model is fed per step (32 ms). */
export const FRAME_SAMPLES = 512;
/** The model's own input also carries the last samples of the step before (v5: 64 at 16 kHz). */
export const CONTEXT_SAMPLES = 64;
export const FRAME_MS = (FRAME_SAMPLES / SAMPLE_RATE) * 1000;
/** Frames that take longer than their own duration for this long switch the call to the energy gate. */
export const SLOW_FALLBACK_MS = 3000;
/** This many inference errors in a row switch the call to the energy gate. */
export const MAX_INFERENCE_ERRORS = 10;
