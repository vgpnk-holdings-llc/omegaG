# xsnapshot app — Round 2 evidence and release trail

**Date:** 2026-07-27  
**Round status:** verified static-site import with release-blocking defects  
**Release status:** blocked; the Round 2 browser audit found release-blocking issues and no production deployment occurred

## Provenance and scope

- Implemented artifact: eight files under `website/` (`index.html`, `style.css`, `main.js`, and five local image/icon assets).
- Archive hash (source ZIP SHA-256): `1d9e3c7ba894d4328b4ce4f6fc85b60a97a0f2cb85b3f810f018bd3e3afdf6d1`.
- Corrected Round 2 audit hash: `2ae5ecc88e0e8956f90d30bb78411bbb7a0492e1282e5f84286c0731ec4fca3a`.
- Source map was revealed after the provisional verdict only for the slot-status-independence selection: proposer `cdx-reviewer-xsnapshot-r2`.
- xask targets: none for every axis and role. There was no xask gate.
- Evidence for the evidence/release-trail axis itself: none; it is a documentation axis that records evidence supplied by the implementation and verification roles.
- No prohibited Gemma, Gemini, or Ollama lane was used. This documentation pass accessed no GPU or browser state.

## Axes, roles, and targets

| Axis | Round 2 role | xask target | Target / result |
|---|---|---|---|
| Artifact provenance and import containment | Reverse-engineering/provenance verifier | None | Keep the checksum-pinned, eight-file static import and limit staging to `website/`. |
| Platform-copy correctness | Repository researcher | None | Keep Linux mapper parity separate from Windows-only Codex runtime and status projection. |
| Interaction and responsive behavior | Browser auditor / interaction reviewer | None | Record desktop/mobile artifacts and block release on the selected-slot invariant defect and missing deterministic assertion. |
| Static-site security boundary | Security reviewer | None | Verify local/static behavior and safe external-link attributes; do not broaden runtime scope. |
| Rust and packaging regression | Regression verifier | None | Preserve Rust and packaging files and retain the 215/215 test baseline. |
| Release governance | Release-control reviewer | None | Require independent hosting/DNS control evidence before deployment. |
| Evidence/release trail | Scribe | None | Produce this scoped, auditable report and commit only intended artifacts. |

## Implemented website

The imported static product page presents omegaG as the preserved DS4CC shortcut mapper, with responsive navigation, hero and attribution copy, a six-state lightbar illustration, a modifier-layer feature summary, controller artwork, platform-qualified specifications, default mappings, and repository/release links. The copy explicitly scopes the optional Codex controller runtime and status-to-lightbar projection to Windows while retaining Linux shortcut-mapper support.

## Evidence

| Evidence | Result |
|---|---|
| Static import | 8 files present; HTML parsed; all 6 checked local references resolved. |
| Desktop screenshot | `docs/reports/assets/xsnapshot-app-r2-desktop.png`; 1440×900; SHA-256 `128c5d8e914fb22ae672911318db6ec35f045fb9ec38501817307df752690f6a`. |
| Mobile screenshot | `docs/reports/assets/xsnapshot-app-r2-mobile.png`; 390×844; SHA-256 `578e4e72b0776920e3a31ec93ea6d52e93f565f8b017277bb13159019277b338`. |
| Rust baseline | `cargo test --locked`: **215/215 passed**, 0 failed, with 3 existing warnings. |
| Change boundary | No intended Rust or packaging changes; release staging is limited to `website/`, the two screenshots, and the Round 1/Round 2 reports. |

The earlier “screenshots absent” observation was a concurrency snapshot; both final artifacts exist with the dimensions and hashes above.

## Security audit

- Static review found no dangerous DOM injection, dynamic network, or browser-storage sinks in the imported HTML/JavaScript.
- All five links using `target="_blank"` also use `rel="noopener noreferrer"`.
- The illustration is explicitly described as non-live; it does not claim a controller, HID, Codex, hosting, or DNS connection.
- No secrets, temporary files, `.xbreed` state, runtime code, packaging files, hosting, or DNS changes belong in the commit.

## Known blocking defects

1. **Selected-slot invariant:** `website/index.html` initially marks slot 3, while `website/main.js` calls `select(chips[1], 1)`, which selects slot 2. Visible selection, initial markup, ARIA state, and etched state therefore lack a single deterministic invariant.
2. **Verification gap:** there is no deterministic DOM assertion proving the selected slot, pressed state, and etched state agree.
3. **Release control:** no hosting or DNS control evidence exists.

These are release-blocking issues. The Round 2 browser audit did not authorize production use, and no production deployment occurred.

## Pareto verdicts

| Move | Verdict | Reason |
|---|---|---|
| `R2-01` static import | **KEEP** | Adds the contained website while preserving Rust and packaging boundaries. |
| `R2-02` selected-slot invariant | **FIX IN ROUND 3** | The one-index mismatch blocks release; add the smallest code correction and an assertion. |
| `R2-03` platform-qualified copy | **KEEP** | Matches the documented Linux/Windows boundary. |
| `R2-04` static security posture | **KEEP** | No reviewed sink or unsafe new-tab link was found. |
| `R2-05` screenshot evidence | **RECORD** | Both final artifacts are present and checksum-pinned. |
| `R2-06` regression/release gate | **KEEP BLOCKED** | Tests pass, but browser defects and absent hosting/DNS proof prevent release. |

**Non-obvious claim (only):** the mismatch is observable after JavaScript initialization because `select(chips[1], 1)` rewrites the selected-slot class to slot 2 even though the initial HTML marks slot 3.

**Rejected alternative (only):** delete the interaction JavaScript. Rejected because a one-index repair preserves the demonstrated interaction at lower scope.

## Mandatory evidence audit

EVIDENCE_AUDIT: 6 moves | 6 valid | 0 spoofed | 1 explicit conflict | 1 rejected alternative | screenshots 2/2 present | SOURCE_MAP withheld.

## Round 3 brief

1. Change the initial selection so the declared slot and executed slot agree.
2. Add a deterministic DOM check for selected slot, `aria-pressed`, and etched state.
3. Re-run static reference/security checks, desktop/mobile browser checks, screenshot hashing, and `cargo test --locked`.
4. Preserve the website-only implementation boundary and stage only intended evidence/report updates.
5. Keep deployment blocked until the browser defects are closed and separate hosting/DNS control evidence is available.

## Round boundary

Round 2 imports and records a static site only. It does not change Rust application/runtime or packaging code, does not mutate hosting or DNS, and does not deploy to production.
