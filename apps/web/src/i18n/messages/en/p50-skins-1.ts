// MUS-SKN-002 (wave 50): the player skins' working extras, D-521 to D-525.
export default {
  // Valve & Walnut: the tape-speed slider's readout and the balance knob.
  'player.skinx.valve.speedValue': '{rate}x',
  'player.skinx.valve.balance': 'Balance L/R',
  'player.skinx.valve.balanceCenter': 'Center',
  'player.skinx.valve.balanceLeft': 'Left {n}%',
  'player.skinx.valve.balanceRight': 'Right {n}%',
  // Night Circuit: the tempo fader beside GAIN (pitch preserved).
  'player.skinx.circuit.tempo': 'Tempo',
  'player.skinx.circuit.tempoValue': '{pct}%',
  // Tidepool Glass: the sleep timer in the status bar.
  'player.skinx.tidepool.sleep': 'Drift off: off',
  'player.skinx.tidepool.sleepIn': 'Drift off: {n} min',
  // Pastel Study Deck: the ambience mixer, synthesised with Web Audio.
  'player.skinx.study.mixerMade': 'Made in your browser, nothing is downloaded.',
} satisfies Record<string, string>;
