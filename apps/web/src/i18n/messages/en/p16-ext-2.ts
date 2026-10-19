export default {
  // Packs need MANAGE_GUILD_EXPRESSIONS alone (D-282): the remove button names what it takes away
  'settings.extensions.removeFromServerCount':
    '{count, plural, =0 {Remove from server} one {Remove from server (# item)} other {Remove from server (# items)}}',
  'settings.extensions.removePackCount':
    '{count, plural, =0 {Remove {name}} one {Remove {name} and the # item it added} other {Remove {name} and the # items it added}}',
};
