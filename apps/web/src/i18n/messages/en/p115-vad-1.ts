// D-1048, D-1050, D-1051 (VOX-CLI-002): the optional on-device voice detector.
export default {
  'settings.voice.vadEngine': 'Voice detection',
  'settings.voice.vadEngineStandard': 'Standard',
  'settings.voice.vadEngineSilero': 'Silero (on-device model)',
  'settings.voice.vadEngineHint':
    'Runs on this device. Nothing is sent anywhere. Downloads about 18 MB the first time.',
  'settings.voice.vadEngineModelDecides':
    'The model decides when you are speaking, so sensitivity does not apply.',
  'settings.voice.vadStatus.idle':
    'Not loaded yet. It loads when you join a call or press "Let\'s check".',
  'settings.voice.vadStatus.loading': 'Loading the model...',
  'settings.voice.vadStatus.ready':
    'Ready. The model is listening to your microphone on this device.',
  'settings.voice.vadStatus.fallback':
    'The model could not start, so standard voice detection is in use. It tries again next time.',
  'voice.vad.fallbackNotice':
    'The Silero voice detector could not start, so this call is using standard voice detection.',
} satisfies Record<string, string>;
