export default {
  // ---- Wave 35 integration: why the Discord bot is not connected (status from GET .../bridge/status) ----
  'settings.bridge.token.invalid':
    'Discord refused the token. Reset it on the Bot page of your application and paste the new one.',
  'settings.bridge.token.disconnected':
    'The connection to Discord dropped; the bridge tries again on its own every half minute.',
} satisfies Record<string, string>;
