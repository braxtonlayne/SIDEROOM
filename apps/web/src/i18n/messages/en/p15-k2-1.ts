// K-21 (D-263): the server requires encrypted calls and this browser cannot join one.
// K-16, K-17, K-18 (D-259, D-260): profile page pictures, uploads and the guestbook count.
// K-20 (D-262): move a Discord identity to another account.
export default {
  'voice.e2ee.required': 'This server requires encrypted calls; your browser cannot join.',
  'lounge.profilePage.guestbookCount':
    '{count, plural, one {# entry} other {# entries}} of {limit}. A guestbook holds at most {limit}; remove old entries to make room.',
  'lounge.profileEditor.backgroundKind': 'Background',
  'lounge.profileEditor.backgroundPattern': 'Pattern only',
  'lounge.profileEditor.backgroundBanner': 'My profile banner',
  'lounge.profileEditor.backgroundImage': 'A picture I uploaded',
  'lounge.profileEditor.backgroundUpload': 'Upload a background picture',
  'lounge.profileEditor.backgroundUploadFile': 'Background picture file',
  'lounge.profileEditor.backgroundRemove': 'Remove the background picture',
  'lounge.profileEditor.galleryUpload': 'Upload a picture',
  'lounge.profileEditor.galleryUploadFile': 'Gallery picture file',
  'lounge.profileEditor.galleryCaption': 'Caption of picture {name}',
  'lounge.profileEditor.galleryRemove': 'Remove picture',
  'lounge.profileEditor.uploadFailed': 'The picture could not be uploaded. Try a PNG, JPEG, GIF or WebP file under 8 MiB.',
} satisfies Record<string, string>;
