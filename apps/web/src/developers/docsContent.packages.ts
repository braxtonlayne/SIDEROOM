// Documentation pages for declarative packages (EXT-PKG-007) and the generated references (DEV-PORT-004): the
// API reference is read from the server's OpenAPI document and the gateway reference is generated from
// docs/05-api-and-gateway.md, so neither can fall behind the code or the specification.
import type { DocPage } from './docsContent';

export const PACKAGE_PAGES: DocPage[] = [
  {
    id: 'api-reference',
    title: 'API reference',
    summary: 'Every endpoint, generated from the server’s OpenAPI document.',
    blocks: [
      {
        t: 'p',
        text: 'This list is read from {origin}/api/v1/openapi.json when you open the page, so it always matches the server you are on. Search by path or description, filter by area, and open an endpoint for its parameters and responses. The interactive reference with a request console is at /developers/docs/api.',
      },
      { t: 'ref', what: 'openapi' },
    ],
  },
  {
    id: 'gateway',
    title: 'Gateway reference',
    summary: 'Opcodes and dispatch events, generated from the specification.',
    blocks: [
      {
        t: 'p',
        text: 'The gateway follows Discord’s numbering and event names. The tables below are generated from docs/05-api-and-gateway.md (sections 6.1 and 6.3); that file also describes intents, session lifecycle, voice and the payload of every SIDE ROOM event.',
      },
      { t: 'ref', what: 'gateway' },
    ],
  },
  {
    id: 'packages',
    title: 'Packages',
    summary:
      'Make a theme, profile layout, pack, recipe or server template and submit it for review.',
    blocks: [
      {
        t: 'p',
        text: 'A package is data, never code: staff review it once per version, and after that anyone can turn it on with one click. Every package is a zip with one manifest file at its root. The Packages page of the portal takes the zip, lists every problem the checks find, and shows the review.',
      },
      { t: 'h', text: 'The kinds' },
      {
        t: 'ul',
        items: [
          'Theme: theme.json, tokens.json, optional assets/ (PNG, WebP, AVIF) and fonts/ (WOFF2 with a licence file). Tokens are --ui-* design tokens; colours are checked for contrast.',
          'Profile layout: layout.json with a "doc" holding blocks on the 12 column grid and a style. Blocks carry no content, only a place and a size.',
          'Emoji, sticker and sound packs: pack.json with a kind and a list of items, and the files under emoji/, stickers/ or sounds/. Emoji up to 256 KiB and 512 px, stickers up to 512 KiB and 1024 px, sounds up to 512 KiB as Ogg or MP3.',
          'Recipe: recipe.json with the feature modules to switch on or off and their settings. A recipe is shared between servers, so it cannot name channels or roles.',
          'Server template: template.json with a serialized_source_guild, the same shape as a server template.',
        ],
      },
      { t: 'h', text: 'Every manifest' },
      {
        t: 'code',
        lang: 'json',
        title: 'recipe.json',
        code: '{\n  "id": "anime-club-starter",\n  "name": "Anime club starter",\n  "version": "1.0.0",\n  "author": "Your name",\n  "description": "Starboard with a lower bar.",\n  "features": [\n    { "id": "lounge.starboard", "enabled": true, "settings": { "threshold": 2 } }\n  ]\n}',
      },
      {
        t: 'ul',
        items: [
          'id is a 3 to 48 character slug (a to z, 0 to 9, hyphen) and is yours: nobody else can submit a package with it. A theme’s id is its data-theme name.',
          'version is MAJOR.MINOR.PATCH. Every submission needs a new version; an update is reviewed again before anyone can move to it.',
          'Unknown fields, files the manifest does not use, hidden files, absolute paths and ".." are refused, and so is a zip over 8 MiB or with more than 400 entries.',
        ],
      },
      { t: 'h', text: 'Check it locally' },
      {
        t: 'code',
        lang: 'sh',
        title: 'sr-cli',
        code: 'sr-cli package validate ./my-package\nsr-cli package validate ./my-package --json',
      },
      {
        t: 'p',
        text: 'The command runs the same checks as the upload for every kind except two that need the running server: whether a recipe’s feature modules exist and accept its settings, and whether a server template’s structure is valid. Those are checked when you submit.',
      },
      { t: 'h', text: 'Review' },
      {
        t: 'ul',
        items: [
          'A submitted version waits in the review queue. Staff can approve it, reject it with a reason you can read, or later revoke it.',
          'You can withdraw a version that is still waiting and submit a fixed one.',
          'Approved packages appear in the Theme Store (themes), in Profile page layouts, and in Server Settings under Extensions (recipes, packs, templates).',
        ],
      },
    ],
  },
];
