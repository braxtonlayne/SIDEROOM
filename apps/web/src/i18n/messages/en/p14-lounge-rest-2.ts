export default {
  // Join pipeline: rules screening and onboarding (LNG-GLD-030, LNG-GLD-031, LNG-GLD-032)
  'lounge.joinGate.rulesTitle': 'Welcome to {name}',
  'lounge.joinGate.rulesIntro': 'Read the rules and agree to them to start talking.',
  'lounge.joinGate.rulesAgree': 'I have read and agree to the rules',
  'lounge.joinGate.rulesSubmit': 'Complete',
  'lounge.joinGate.rulesError': 'Could not save your answer. Try again.',
  'lounge.joinGate.notNow': 'Not now',
  'lounge.joinGate.onboardingTitle': 'Set up your time in {name}',
  'lounge.joinGate.onboardingIntro':
    'Pick what interests you and we will show the matching channels.',
  'lounge.joinGate.onboardingSubmit': 'Continue',
  'lounge.joinGate.onboardingError': 'Could not save your choices. Try again.',
  'lounge.joinGate.required': '(required)',
  'lounge.joinGate.skip': 'Skip',
  'lounge.joinGate.browseTitle': 'Your channels',
  'lounge.joinGate.browseIntro': 'These are the channels you will see in {name}.',
  'lounge.joinGate.done': 'Done',

  // Create Server: icon step (LNG-GLD-017)
  'lounge.addServerDialog.iconPreview': 'Server icon preview',
  'lounge.addServerDialog.iconUpload': 'Upload an icon',
  'lounge.addServerDialog.iconChange': 'Change the icon',
  'lounge.addServerDialog.iconRemove': 'Remove the icon',
  'lounge.addServerDialog.iconFile': 'Server icon file',
  'lounge.addServerDialog.cropTitle': 'Crop the server icon',

  // Server guide (LNG-GLD-033)
  'lounge.channelSidebar.serverGuide': 'Server Guide',
  'lounge.serverGuide.title': '{name} guide',
  'lounge.serverGuide.close': 'Close',
  'lounge.serverGuide.loading': 'Loading the guide',
  'lounge.serverGuide.error': 'Could not load the guide.',
  'lounge.serverGuide.empty': 'The managers have not written a guide yet.',
  'lounge.serverGuide.todos': 'Things to do',
  'lounge.serverGuide.resources': 'Resources',
  'lounge.serverGuide.goTo': 'Go to #{name}',

  // Security actions (LNG-GLD-034)
  'lounge.securityActions.title': 'Security Actions',
  'lounge.securityActions.subtitle': 'Quick switches for {name} while something is going wrong.',
  'lounge.securityActions.close': 'Close',
  'lounge.securityActions.loading': 'Loading the current settings',
  'lounge.securityActions.loadError': 'Could not load the security settings.',
  'lounge.securityActions.saveError': 'Could not change that setting.',
  'lounge.securityActions.pauseInvites': 'Pause invites',
  'lounge.securityActions.pauseInvitesHint':
    'Nobody can join with an invite link until you turn this off.',
  'lounge.securityActions.pauseDms': 'Pause direct messages from members',
  'lounge.securityActions.pauseDmsHint':
    'Messages from members of this server go to Message Requests instead of the inbox.',
} satisfies Record<string, string>;
