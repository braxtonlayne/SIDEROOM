export default {
  // ---- Person links and suspicion data (phase 19 integration round, D-349 to D-351) ----
  // A signal a Person link folded: {id} is the Person that was merged into the one shown.
  'admin.suspicion.signalFolded': 'Folded (Person {id})',
  // A once-automatic case of a Person that a link merged away; the link names the surviving automatic case.
  'admin.suspicion.caseDemoted': 'Case {id}, from a merged Person:',
  'admin.suspicion.caseFoldedInto': 'folded into case {id}',
} satisfies Record<string, string>;
