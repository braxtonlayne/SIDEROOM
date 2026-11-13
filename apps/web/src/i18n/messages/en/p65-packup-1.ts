// SEC-AUD-177 (wave 65): the Activity package upload on the developer page (D-654, D-655 as shown to the developer).
export default {
  'developers.packageUpload.limitStandard':
    'Packages up to {max}. Reviewed applications can upload more.',
  'developers.packageUpload.limitTier': 'Packages up to {max} ({tier}).',
  'developers.packageUpload.tierStandard': 'standard limit',
  'developers.packageUpload.tierReviewed': 'reviewed application',
  'developers.packageUpload.tierPlatform': 'platform application',
  'developers.packageUpload.overLimit':
    'That file is {size}. The limit for this application is {max} ({tier}). The operator of this server can raise the limit.',
  'developers.packageUpload.noParts':
    'This server\u2019s storage cannot take large packages. A package of up to 32 MiB can still be uploaded.',
  'developers.packageUpload.sent': '{sent} of {total} sent',
  'developers.packageUpload.checking': 'Checking the package...',
  'developers.packageUpload.checked': '{done} of {total} checked',
  'developers.packageUpload.cancel': 'Cancel upload',
  'developers.packageUpload.cancelled': 'The upload was cancelled.',
  'developers.packageUpload.resumed': 'Continuing the upload of {name}...',
  'developers.packageUpload.failed': 'The upload failed. Choose the file again to retry.',
  'developers.packageUpload.checkFailed': 'The package could not be checked. Try again.',
  'developers.packageUpload.expired': 'That upload expired. Choose the file again.',
  'developers.packageUpload.gone': 'The server no longer has that upload. Choose the file again.',
} satisfies Record<string, string>;
