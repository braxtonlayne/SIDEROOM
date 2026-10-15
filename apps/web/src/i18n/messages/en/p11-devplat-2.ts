export default {
  // Developer portal: interactions endpoint card (General tab)
  'developers.endpoint.heading': 'Interactions endpoint',
  'developers.endpoint.intro':
    'Instead of the gateway, SIDE ROOM can send every interaction to your server as a signed HTTPS request. Your server checks the signature with the public key below and answers with the response body of an interaction callback.',
  'developers.endpoint.publicKey': 'Public key',
  'developers.endpoint.publicKeyHint':
    'Ed25519, hex. Verify the X-Signature-Ed25519 and X-Signature-Timestamp headers against the timestamp followed by the raw body.',
  'developers.endpoint.noKey': 'No key yet',
  'developers.endpoint.copyKey': 'Copy public key',
  'developers.endpoint.createKey': 'Create public key',
  'developers.endpoint.keyFailed': 'Could not create the key.',
  'developers.endpoint.urlLabel': 'Interactions endpoint URL',
  'developers.endpoint.urlHint':
    'Saving sends a signed PING, which must be answered with a PONG (type 1), and a PING with a wrong signature, which must be refused with 401.',
  'developers.endpoint.errHttps': 'The URL must start with https://.',
  'developers.endpoint.errInvalid':
    'Not a valid URL (no spaces, credentials or fragment; at most 512 characters).',
  'developers.endpoint.save': 'Verify and save',
  'developers.endpoint.remove': 'Remove endpoint',
  'developers.endpoint.saved': 'Endpoint verified and saved.',
  'developers.endpoint.removed': 'Endpoint removed. Interactions use the gateway again.',
  'developers.endpoint.saveFailed': 'Could not verify that endpoint.',
} satisfies Record<string, string>;
