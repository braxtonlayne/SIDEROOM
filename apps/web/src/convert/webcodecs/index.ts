// BYOS-CNV-002: public surface of the WebCodecs path. conv-remux's Converter
// imports from here and from `../decide`; nothing else in this folder is API.
//
//   const caps = detectCapabilities();
//   const plan = await decide(info, caps);           // native | remux | webcodecs | worker
//   if (plan.path === 'webcodecs') {
//     // 1. ffmpeg.wasm: exec(['-i', input, '-map', '0:v:0?', '-map', '0:a:0?', ...FRAGMENT_ARGS, 'frag.mp4'])
//     // 2. const bytes = await ffmpeg.readFile('frag.mp4');
//     const source = await demuxFmp4(chunked(bytes), info.durationSec);
//     const out = await transcodeWithWebCodecs({ source, plan, signal, onProgress });
//     // out.blob (memory) is the playable file; upload it as the converted object.
//   }

export { decide, describeDecision, detectCapabilities, estimateWasmMinutes } from '../decide';
export { Fmp4Demuxer, chunked, demuxFmp4 } from './fmp4';
export { transcodeWithWebCodecs } from './transcode';
export * from './types';

/**
 * ffmpeg.wasm arguments that turn any container into the fragmented MP4 our
 * demuxer reads, copying packets (no re-encode). Place them after the `-map`
 * options and before the output name.
 */
export const FRAGMENT_ARGS: readonly string[] = [
  '-c',
  'copy',
  '-f',
  'mp4',
  '-movflags',
  'frag_keyframe+empty_moov+default_base_moof',
];
