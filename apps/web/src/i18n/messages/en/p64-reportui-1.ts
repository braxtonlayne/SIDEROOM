// SEC-AUD-168 (D-647..D-650): the beta bug reporter's dialog and Admin > Bug reports.
export default {
  'bugreport.shortcut': 'Report a bug',
  'bugreport.menuEntry': 'Report a bug',
  'bugreport.title': 'Report a bug',
  'bugreport.intro':
    'Tell the staff of this SIDE ROOM what went wrong. This is the beta; thank you for helping.',
  'bugreport.descriptionLabel': 'What went wrong?',
  'bugreport.counter': '{n} / {max}',
  'bugreport.pictureTitle': 'Screenshot',
  'bugreport.pictureLabel': 'Your screenshot. Drag on it to black out a region.',
  'bugreport.noPicture': 'No screenshot will be sent.',
  'bugreport.coverHint':
    'Drag on the picture to black out anything private. Each black box has a remove button.',
  'bugreport.autoCovered':
    'Direct messages and calls on this page are already blacked out. You can remove a box.',
  'bugreport.checkPicture': 'Check the picture: black out anything private before you send it.',
  'bugreport.removeCover': 'Remove black box {n}',
  'bugreport.removePicture': 'Remove screenshot',
  'bugreport.capture': 'Capture this tab',
  'bugreport.captureAgain': 'Capture this tab again',
  'bugreport.attach': 'Attach a screenshot',
  'bugreport.noteDeclined':
    'The browser did not share the tab, so there is no picture. You can try again or attach a screenshot.',
  'bugreport.noteFailed':
    'The picture could not be taken. You can try again or attach a screenshot.',
  'bugreport.noteUnsupported':
    'This browser cannot capture the tab. You can attach a screenshot taken with your device.',
  'bugreport.whatIsSent': 'What is sent',
  'bugreport.whatIsSentIntro':
    'Your description and the picture above are sent with exactly this information about your browser. It holds no messages, links, tokens or ids.',
  'bugreport.jsonLabel': 'The debug information that is sent',
  'bugreport.privacy':
    'Staff of this SIDE ROOM can read this report. The screenshot is deleted when the report is closed or after 30 days, the report after 90 days.',
  'bugreport.send': 'Send',
  'bugreport.cancel': 'Cancel',
  'bugreport.close': 'Close',
  'bugreport.sentTitle': 'Report sent',
  'bugreport.sentBody': 'Thank you. Report #{n} was sent.',
  'bugreport.errorLimit': 'You have sent several reports lately. Please try again in an hour.',
  'bugreport.errorTooLarge':
    'The report is too large. Remove the screenshot or send a smaller one.',
  'bugreport.errorPicture': 'The picture is too large to send. Remove it or attach a smaller one.',
  'bugreport.errorOffline': 'You seem to be offline. Check your connection and try again.',
  'bugreport.errorOff': 'Bug reports are switched off right now.',
  'bugreport.errorFile': 'That file could not be used. Attach an image, 30 MB at most.',
  'bugreport.errorGeneric': 'The report could not be sent. Please try again.',
  'help.handbook.bugreport.title': 'Reporting a bug (beta)',
  'help.handbook.bugreport.what':
    'While the beta is on, the operator can switch on a bug reporter. Press Alt+Shift+B (you can change the key in Keybinds), or choose Report a bug in the ? menu, or in the You tab on a phone. You describe what went wrong, see the exact picture and information that will be sent, and press Send.',
  'help.handbook.bugreport.picture':
    'The picture is taken by your browser, which asks you every time. On a direct message, a group message or in a call the messages, the message box and the video start blacked out; you can drag on the picture to black out more, remove a box, or remove the picture. Nothing is sent until you press Send.',
  'help.handbook.bugreport.limit1':
    'Your browser asks to share the tab each time you take a picture.',
  'help.handbook.bugreport.limit2':
    'Firefox and Safari show their own picker, and you can pick another window; the preview shows what was picked, so check it.',
  'help.handbook.bugreport.limit3':
    'Phones cannot capture the tab: attach a screenshot taken with the phone instead, or send the report without one.',
  'help.handbook.bugreport.limit4':
    'The picture shows what is visible, not the whole scrolled page.',
  'help.handbook.bugreport.kept':
    'The staff of this SIDE ROOM can read a report. The screenshot is deleted when the report is closed or after 30 days, and the report after 90 days. You can send at most 5 reports an hour.',
  'help.handbook.bugreport.link': 'Keybinds',
  'help.operators.betaBody':
    'Off by default. Turn it on in Admin > Features for the beta (under 100 members) so members can send a bug report with a screenshot and debug information to the staff; turn it off when the beta ends. Admin > Bug reports lists the reports, and every opened report and every shown screenshot is written to the audit log.',
} satisfies Record<string, string>;
