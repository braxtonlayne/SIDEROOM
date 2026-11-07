// SEC-AUD-032 (wave 54, D-568): a cartridge's tier comes from its sideroom.json, not from the lister.
export default {
  'arcade.cartridges.tierFromManifest':
    'The content tier and capabilities are read from the game’s sideroom.json when it is listed.',
} satisfies Record<string, string>;
