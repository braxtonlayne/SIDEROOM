// @ts-check
// Open Field: the plain square every arena used before maps. No walls, no gold.

/** @type {import('./index.js').MapDef} */
export default {
  id: 'open',
  name: 'Open Field',
  blurb: 'The plain square. Nowhere to hide.',
  build(w, h) {
    return new Uint8Array(w * h);
  },
};
