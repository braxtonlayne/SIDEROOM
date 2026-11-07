// SEC-AUD-042 (wave 55, D-574): a perk is never granted to the staff member's own account.
export default {
  'admin.entitlements.grantDialog.ownAccount':
    'You cannot grant a perk to your own account. Another staff member has to do it.',
} satisfies Record<string, string>;
