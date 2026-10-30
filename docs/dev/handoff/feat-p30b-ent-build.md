# Handoff: feat/p30b-ent-build (SEC-ENT-001)

## Resume here
Slice 1 done: `apps/web/public-surfaces.json` and the dist contract below. Next: the two builds, the resolveId guard, landing copy, help behind login, enterApp, csp, render-legal, split-notices, budgets.

## Dist layout contract (fixed by entrance-isolation.md; not renegotiated)
```
dist/
  index.html auth.html invite.html [profile.html later]   public shells (one per bundle surface)
  entrance/…                                              public build output: hashed JS/CSS, legal-*.css, door image, .vite/manifest.json
  fonts/ icons/ sw.js manifest.webmanifest offline.html offline.css robots.txt 404.html 50x.html   public files (from apps/web/public)
  THIRD_PARTY_NOTICES.md                                  public copy (split-notices.mjs)
  legal/house-notes.<locale>.html                         templates with {{operator_country}} / {{operator_contact}}
  surfaces.json                                           copy of apps/web/public-surfaces.json (reserved surfaces keep "reserved": true and have no shell)
  app.html                                                the gated app shell (CSP/robots handled by the server)
  app/                                                    gated: hashed assets, manifest.webmanifest, THIRD_PARTY_NOTICES.md (full)
  app/csp.txt                                             the app CSP, one line, sent verbatim by sr-server
  app/routes.json                                         JSON array of the app router's route paths (from routePaths() in router.tsx)
```
Vite base of the public build is `/entrance/`; of the app build `/app/`.
