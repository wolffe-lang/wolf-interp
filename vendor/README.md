# vendor/upstream

A tracked snapshot of `spec/` + `corpus/` from wolf-lang at the commit
recorded in `PIN` — byte-identical to the `upstream/` submodule at that
pin. Exists because the submodule is private and CI cannot clone it
(deploy keys are disabled org-side); the snapshot keeps CI hermetic.

Rules:
- NEVER edit files here by hand; re-vendor on pin bumps:
  `git -C upstream fetch && git -C upstream checkout <sha> &&
   rm -rf vendor/upstream/{spec,corpus} &&
   cp -r upstream/{spec,corpus} vendor/upstream/ &&
   git -C upstream rev-parse HEAD > vendor/upstream/PIN`
- The `vendor_matches_submodule` test verifies snapshot == submodule
  whenever the submodule is initialized (always true locally; skipped
  in CI where the submodule is absent).
- Retire this directory when the upstream repo becomes readable to CI
  (repo public at v1, or deploy keys enabled).
