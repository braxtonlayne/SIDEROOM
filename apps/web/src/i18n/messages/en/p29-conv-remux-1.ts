// Convert step of the upload dialogs (BYOS-CNV-001): remux and subtitle extraction in the browser.
export default {
  'convert.step.title': 'Make these files playable',
  'convert.step.intro':
    '{count, plural, one {This file is} other {These files are}} not in a format browsers play. Your browser can repackage {count, plural, one {it} other {them}} without re-encoding, so the picture and sound stay exactly as they are. Nothing is sent to the server for this.',
  'convert.step.extractSubtitles': 'Also pull out the subtitles as separate files',
  'convert.step.start': 'Convert in this browser',
  'convert.step.asIs': 'Upload as they are',
  'convert.step.cancel': 'Cancel',
  'convert.step.stop': 'Stop converting',
  'convert.step.required':
    'This server only accepts files that play in a browser, so they have to be converted first.',
  'convert.step.list': 'Files to convert',
  'convert.step.state.waiting': 'Waiting',
  'convert.step.state.working': 'Converting',
  'convert.step.state.done': 'Done',
  'convert.step.state.failed': 'Failed',
  'convert.step.state.skipped': 'Already plays, left as it is',
  'convert.step.done':
    '{count, plural, one {# file is} other {# files are}} ready. Continue to upload.',
  'convert.step.continue': 'Continue',
  'convert.step.subtitles': '{count, plural, one {# subtitle file} other {# subtitle files}}',
  'convert.step.bitmapSubtitles':
    '{count, plural, one {# picture-based subtitle track} other {# picture-based subtitle tracks}} cannot be turned into text and was left out.',
  'convert.progress.loading': 'Loading the converter (about 30 MB, once)',
  'convert.progress.probing': 'Reading the file',
  'convert.progress.remuxing': 'Repackaging',
  'convert.progress.extracting': 'Extracting subtitles',
  'convert.progress.transcoding': 'Converting the video',
  'convert.progress.uploading': 'Uploading',
  'convert.progress.percent': '{name}: {phase}, {percent}%',
  'convert.progress.working': '{name}: {phase}',
  'convert.progress.eta': 'about {minutes, plural, one {# minute} other {# minutes}} left',
  'convert.progress.etaSeconds': 'less than a minute left',
  'convert.error.tooLarge':
    'This file is over {limit} MiB, more than a browser tab can convert. Ask your server operator about a transcoding worker, or convert it on your own computer first.',
  'convert.error.needsTranscode':
    'This file cannot be made playable by repackaging alone ({reason}). A transcoding worker or a converter on your own computer is needed.',
  'convert.error.unsupported': 'The converter could not read this file.',
  'convert.error.engine': 'The converter stopped: {detail}',
  'convert.error.noOutput': 'The converter produced no file.',
  'convert.error.cancelled': 'Stopped.',
  'convert.error.noEngine': 'The converter could not be loaded.',
  'convert.reason.noVideo': 'no video',
  'convert.reason.videoCodec': 'the video format',
  'convert.reason.audioCodec': 'the sound format',
} satisfies Record<string, string>;
