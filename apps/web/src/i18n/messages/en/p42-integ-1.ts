// Wave 42 integration (ARC-ROOM-001): a Guild Cartridge on a shelf object (wave 41).
export default {
  'arcade.cartridges.open': 'Get {title} from the shelf',
  'arcade.cartridges.openFailed':
    'This file could not be opened. You may not have access to the shelf it is on.',
} satisfies Record<string, string>;
