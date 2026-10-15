// Developer portal: tester management and endpoint health (G-01 D-204, G-05 D-186), and the tester invitation
// card answered from a system DM.
export default {
  'developers.testers.heading': 'Testers',
  'developers.testers.intro':
    'Invite up to {max} people to test this app. Each gets a direct message from SIDE ROOM with Accept and Decline. Accepted testers can add the app to servers they manage that have 25 members or fewer, like you.',
  'developers.testers.username': 'Username',
  'developers.testers.usernamePlaceholder': 'username',
  'developers.testers.invite': 'Invite tester',
  'developers.testers.invited': 'Invitation sent to {name}.',
  'developers.testers.inviteFailed': 'Could not send the invitation.',
  'developers.testers.removeFailed': 'Could not remove that tester.',
  'developers.testers.full':
    'This app already has {max} testers. Remove one to invite someone else.',
  'developers.testers.testers': 'testers',
  'developers.testers.unavailable': 'Testers are not available on this server.',
  'developers.testers.none': 'No testers yet.',
  'developers.testers.listLabel': 'Testers',
  'developers.testers.stateAccepted': 'Accepted',
  'developers.testers.stateInvited': 'Invited',
  'developers.testers.remove': 'Remove',
  'developers.testers.withdraw': 'Withdraw',
  'developers.testers.removeLabel': 'Remove {name} as a tester',
  'developers.testers.answerAccepted': 'You are now a tester of {name}.',
  'developers.testers.answerDeclined': 'Invitation declined.',

  'developers.endpoint.disabledTitle': 'Endpoint switched off',
  'developers.endpoint.disabledBody':
    'Your server failed 25 interactions in a row within ten minutes, so SIDE ROOM stopped sending them to the URL. Interactions use the gateway again. Fix your server, then verify the endpoint to turn it back on.',
  'developers.endpoint.reenable': 'Verify and re-enable',
  'developers.endpoint.reenabled': 'Endpoint verified and switched on again.',
  'developers.endpoint.reenableFailed': 'Could not verify that endpoint.',
  'developers.endpoint.active': 'Active: interactions are sent to this URL.',
} satisfies Record<string, string>;
