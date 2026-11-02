// FU-STU-5 (D-390): the PlayStation core runs on its own open-source HLE BIOS when the member mounts no BIOS.
export default {
  'games.hle.title': "Running on the core's built-in BIOS",
  'games.hle.body':
    "No BIOS file of yours is in use, so this game runs on the emulator's built-in HLE BIOS (open source, nothing of Sony's). Some games need a real BIOS from the emu/bios folder of your own bucket: choose one in the BIOS panel below.",
  'games.bios.hleHint':
    "Without a BIOS file, PlayStation games run on the emulator's built-in HLE BIOS. Some games need a real one.",

  // FU-ARC-5: tip QR codes in the profile page editor.
  'profile.editor.tipQrList': 'Tip links as QR codes',
  'profile.editor.tipQrLabel': 'QR code for {label}',
};
