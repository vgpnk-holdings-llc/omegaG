# xsnapshot app — Round 1 evidence and release trail

**Date:** 2026-07-27  
**Round status:** pre-implementation evidence only  
**Release status:** blocked; no production deployment occurred

## Provenance

- Corrected synthesis: `.xbreed/mailbox/events.ndjson`, discovery from `cdx-distiller-xsnapshot-r1-corrected`; it supersedes synthesis `50187468a99f73c5ca5046dfbb064d922229fcd883a4d7d05d24c7774215aefe`.
- Audit hash: `cd55d7f0cbc382381912856dd70453bf15059d12f621a16831ba8e464302380b`.
- Archive hash (ZIP SHA-256): `1d9e3c7ba894d4328b4ce4f6fc85b60a97a0f2cb85b3f810f018bd3e3afdf6d1`.
- The corrected synthesis withholds its teammate source map. Therefore this report attributes its rows to the corrected distiller rather than inventing individual teammate identities.
- xask targets: none. This documentation axis had no xask gate.

## Axes and roles

| Axis | Round role | Release target |
|---|---|---|
| Artifact provenance and import containment | Reverse-engineering/provenance analysis | Audited, checksum-pinned staging into `website/` |
| Platform-copy accuracy | Repository research | Linux/Windows claims match documented boundaries |
| Accessibility and responsive behavior | Browser-harness evidence | Repeatable desktop/mobile checks against the imported site |
| Rust and packaging preservation | Regression gate | No changes outside `website/` |
| Hosting and DNS control | Release governance | Deployment waits for separate control evidence |
| Evidence/release trail | Scribe | Auditable pre-implementation report and scoped commit |

## Corrected teammate synthesis

Source teammate/role for every row: `cdx-distiller-xsnapshot-r1-corrected` / corrected evidence distiller. `EVIDENCE` values are retained at their stated scope.

| MOVE | AXIS | CLAIM | EVIDENCE | CONFIDENCE |
|---|---|---|---|---|
| `XS-R1C-001_IMPORT` | Artifact provenance/import | Stage the audited eight-entry artifact only after checksum, archive-integrity, containment, link, duplicate, and case-fold-collision checks; do not claim authenticated producer provenance. | None — reverse-engineering artifact/provenance analysis; archive literals independently spot-checked. | Medium |
| `XS-R1C-002_COPY` | Platform-copy accuracy | Qualify WSL, SendInput, EXE, APPDATA, optional Codex runtime, and status-projection language as Windows-only while retaining Linux shortcut-mapper parity. | None — research axis; `README.md:28,48-60,362-450` and `SPEC.md:1-6` were spot-checked by the corrected synthesis. | Medium |
| `XS-R1C-003_A11Y_RESPONSIVE` | Accessibility/responsive harness | Use fresh-profile, GPU-disabled Chromium at desktop and mobile dimensions, then assert semantics, keyboard use, focus, contrast, alternative text, and overflow after import. | Corrected synthesis records Chromium exit 0 at 1440×1000 and 390×844, exact screenshot dimensions, captured DOM, and zero console-error matches against the GitHub repository route. This proves harness capability only; Playwright was unavailable (exit 127). | Medium |
| `XS-R1C-004_RUST_BOUNDARY` | Regression boundary | Constrain future executor changes to `website/` and require unchanged Rust and packaging paths. | Corrected synthesis records `cargo test --locked`: 215 passed, 0 failed, exit 0; Windows-target check blocked because the target was absent. Local report verification also ran `cargo test`: 215 passed, 0 failed, exit 0. | Medium |
| `XS-R1C-005_RELEASE_BOUNDARY` | Hosting/DNS governance | Do not mutate DNS or hosting, and do not portray the static demo as live controller integration, until separate control evidence exists. | None — cross-axis analysis; archive and repository excerpts were spot-checked by the corrected synthesis. | Medium |

**Rejected alternative (the only recorded rejection):** drop the security proposal because it lacked the required verbatim gate/test evidence and proposed updater Rust hardening outside the preserved `website/`-only scope. Spoof flag: false.

## Conflicts and resolutions

**CONFLICT: the archive places Linux parity beside an unqualified Codex runtime — my position: qualify the optional runtime and status projection as Windows-only — peer: the archive wording leaves them unqualified.** Resolution: use the repository boundary in `README.md:429-450`, `SPEC.md:3-6`, and `src/config.rs:343-348`.

Legacy Windows-only onboarding versus later first-class Linux documentation is resolved by separating legacy onboarding copy from current platform support. HEAD containing no tracked static site and an external ZIP existing are compatible facts, not a conflict.

## Pareto verdicts and optimization routes

All five corrected moves are retained: each improves provenance, accuracy, reproducibility, regression safety, or release control without requiring a production mutation in this round.

1. **Safe import route:** verify the archive hash and integrity; reject unsafe or ambiguous entries; stage only `website/` paths.
2. **Truthful-copy route:** separate Linux shortcut-mapper support from Windows-only runtime/status behavior.
3. **Quality route:** rerun the proven Chromium harness against the imported `website/`, then add accessibility and overflow assertions.
4. **Low-regression route:** compare pre/post paths and reject changes outside `website/`; retain the 215/215 Rust baseline.
5. **Release route:** obtain hosting and DNS control evidence independently before any deployment action.

## Gates, spoof audit, and blockers

- Baseline: `cargo test` completed with **215/215 passed**, 0 failed, exit 0 on 2026-07-27; three existing dead-code warnings were emitted.
- Browser harness capability exists per corrected synthesis, but it has not tested an imported site because no site has been imported. This report did not access GPU or browser state.
- Hosting/DNS blocker: control proof is absent. DNS and hosting must remain unchanged.
- Windows-target gate remains blocked by the missing `x86_64-pc-windows-gnu` target in the corrected synthesis.
- Spoof audit: 0 spoof-flagged moves. The synthesis reports matching repository excerpts and archive hashes/lines; browser evidence is scoped only to harness capability.
- Evidence audit from corrected synthesis: 5 moves with valid evidence status, 1 proposal without required evidence, and 1 proposal dropped.

## Round boundary

This report records evidence and release gates only. Round 1 imported no website artifact, changed no application/runtime/packaging code, mutated no hosting or DNS, and performed no production deployment.
