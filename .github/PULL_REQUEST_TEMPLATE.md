# Summary

<!-- What changes and why, in 2-3 sentences -->

## Related issue

Closes #

## Changes

- 

## How to test

<!-- Commands or steps to verify the change locally -->

## Checklist

- [ ] `cargo check` and `cargo test` pass **natively** (never `--target wasm32-*`)
- [ ] No new warnings in our own code (warnings from the vendored FBW code are not ours to fix)
- [ ] Vendor pin unchanged — or, if changed, a new entry is recorded in `docs/decisiones.md` (benchmark reproducibility depends on the pin)
- [ ] No patches to the vendored FBW code — or, if any, documented under "Parches al código vendorizado" in `docs/decisiones.md`
- [ ] Any architecture decision taken here is recorded in `docs/decisiones.md`
- [ ] Docs updated if the change affects the API contract or the phase status

<!-- Reminder: this project links the FBW crates and is therefore GPLv3. -->

## Notes for the reviewer

<!-- Anything worth knowing before reading the diff -->
