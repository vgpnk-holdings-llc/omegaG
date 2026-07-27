# xsnapshot app — orchestration Round 1 superseding report (R3)

**Date:** 2026-07-27

**Round status:** Round 3 verification gates closed; this report supersedes the Round 1/Round 2 project status without rewriting either historical report

**Release status:** locally verified and commit-integrated by the selective commit containing this report; production deployment remains blocked on hosting/DNS authority

## Axes, roster, and xask targets

| Axis | Roster assignment | xask target | R3 target / disposition |
|---|---|---|---|
| Artifact provenance and containment | Reverse-engineering/provenance verifier | None | Retain checksum-pinned source boundary and safe paths; do not claim authenticated producer provenance. |
| Public-copy correctness | Repository researcher | None | Reconcile every public platform/config/runtime/default claim to `README.md` and `SPEC.md`. |
| Responsive and interaction verification | Browser auditor / interaction reviewer | None | Close the selected-slot defect and verify desktop/mobile behavior and artifacts. |
| Static security boundary | Security reviewer | None | Preserve static-only behavior, safe new-tab attributes, and zero browser/network errors. |
| Rust and packaging regression | Regression verifier | None | Preserve non-website code and pass all 215 locked tests. |
| Change integration | Site executor | None | Commit only the reviewed website delta, two R3 screenshots, and this report. |
| Release governance | Release-control reviewer | None | Keep deployment blocked until independent hosting/DNS authority exists. |
| Synthesis | Evidence distiller | None | Reconcile seven move IDs and issue the canonical audit hash. |
| Evidence/release trail | `ccs-scribe-r1` | None | Record reproducible evidence and the selective commit. |

Disclosed evidence producers are `ccs-distiller-r1` and `cdx-executor-site-r1`; this scribe is `ccs-scribe-r1`. Other rows retain role attribution because no fuller authenticated teammate map was available. There was no xask gate on any axis.

## Provenance, archive boundary, and safe paths

- Source ZIP SHA-256: `1d9e3c7ba894d4328b4ce4f6fc85b60a97a0f2cb85b3f810f018bd3e3afdf6d1`.
- The audited source boundary was eight regular files: `index.html`, `style.css`, `main.js`, and five local image/icon assets. Entries were relative, contained beneath `website/`, and had no absolute path, `..` traversal, symlink, duplicate, or case-fold collision.
- The source ZIP is not present in this checkout, so its checksum and archive-path audit are inherited, report-backed facts from the immutable Round 1/Round 2 trail, not a new producer-authentication claim.
- R3 removes the unused `website/assets/logo-badge.png`, optimizes the three used PNGs, and adds `website/checks.mjs`; the resulting static tree remains contained under `website/`.
- Canonical synthesis: `audit_hash=cffccc96b63b7cb0ecbada7c658074250a6a88402e270fc99625a18b189c1cdb`.

## Claim reconciliation

| Public claim | Repository authority | Resolution |
|---|---|---|
| `ds4cc` package/binary, MIT attribution, and Windows config compatibility are preserved | `README.md:24-28,46-52,103-106` | Copy now says **Windows config path** rather than implying one cross-platform literal path. |
| Linux provides shortcut-mapper parity over USB/Bluetooth; Codex runtime and status projection remain Windows-only | `README.md:362-365,429-451`; `SPEC.md:3-6` | Mapper parity and optional runtime behavior are explicitly separated. |
| Linux config honors XDG with a home fallback | `SPEC.md:142-145`; `README.md:453-459` | Site states `$XDG_CONFIG_HOME/ds4cc/config.toml`, falling back to `~/.config/ds4cc/config.toml`. |
| Share/Options defaults and touchpad-button behavior | `README.md:64-99` | Generic-map copy names the unmapped defaults and touchpad-mode condition. |
| Voice/PTT and keyboard-free language require configuration | `README.md:139-164,166-191,268-275,419-421` | Site uses “when configured” / “optional when configured” instead of unconditional claims. |
| Release link destination is upstream DS4CC | `README.md:46-52,310-315` | Link label is “Upstream DS4CC releases.” |

## Browser evidence — HYPOTHESIS / METHOD / RESULT

**HYPOTHESIS:** The R3 site keeps selected slot 3 invariant while state activation updates only status, remains usable at 1440×900 and 390×844, resolves its assets/anchors, and emits no console, page, or network errors.

**METHOD:** Serve the working `website/` over loopback and run Chromium 150 with separate fresh profiles and reduced-motion emulation. At both exact viewports, wait for complete/settled rendering; exercise click-only state activation and keyboard focus; inspect selected-slot, `aria-pressed`, etched/live-region state, landmarks, IDs, in-page anchors, responsive navigation, horizontal overflow, intrinsic/rendered image ratios, and no-JavaScript legibility. Capture PNGs, inspect their dimensions, hash their bytes, collect severe console entries, page/runtime exceptions, failed requests and bad responses, and fetch every local asset plus all three external GitHub destinations.

**RESULT:** PASS with no harness failures. Desktop and mobile each had **0 console errors, 0 page errors, and 0 network errors**. Slot 3 remained selected; initial status was `thinking`; activating `error` changed the pressed, etched, and polite live-region state without changing the slot. No horizontal overflow was found, all in-page targets/IDs and keyboard focus checks passed, all local assets returned 200 with expected MIME types, and the three external links returned 200 (the release link resolved to DS4CC `v3.1.0`).

| Artifact | Exact dimensions | SHA-256 |
|---|---:|---|
| `docs/reports/assets/xsnapshot-app-r3-desktop.png` | 1440×900 | `271c8e7fc8ee075675a022ed6895059264981655960334fd0a3aa1aeaef6d228` |
| `docs/reports/assets/xsnapshot-app-r3-mobile.png` | 390×844 | `095d2c039406eddf9b65120803cf45efd0f5e770c2f30eec2ba7c58f31fb9aca` |

## Deterministic and regression evidence

- `node website/checks.mjs` → `website checks: pass`. It deterministically asserts slot 3, one pressed state, etched/live status agreement, activation-only updates, claim literals, responsive image metadata, local references, and removal of the unused badge.
- `cargo test --locked` → **215 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out**; exit 0. The same three existing dead-code warnings remain.
- `git diff --check` → exit 0, no output.
- PNG verification with `file` and `sha256sum` reproduced the exact dimensions and hashes above.
- Asset/link/reference verification → all six browser-loaded local resources returned 200; all in-page anchors resolved; all five `_blank` links retain `rel="noopener noreferrer"`; all three distinct external destinations returned 200.

## Round 2 mandatory status

1. **Selected-slot agreement — CLOSED:** markup fixes slot 3 as `aria-current`; JavaScript no longer rewrites slot selection.
2. **Deterministic DOM assertion — CLOSED:** `website/checks.mjs` proves selected slot, pressed state, etched state, and live state before and after activation.
3. **Static/browser/screenshots/Rust rerun — CLOSED:** deterministic checks, final Chromium report, exact PNG hashes, references, and 215/215 locked tests pass.
4. **Website-only implementation boundary/selective staging — CLOSED by this commit:** no Rust, packaging, `.xbreed/`, or unrelated path is included.
5. **Hosting/DNS authority — STILL MANDATORY / BLOCKED:** no independent control evidence exists; no hosting or DNS mutation and no production deployment occurred.

## Conflicts and resolution

**CONFLICT:** local deterministic/browser evidence supports correctness readiness; the integration and deployment gates were previously still blocked. **Resolution:** integrate the verified working-tree state through the selective commit containing this report, but keep deployment blocked because local correctness does not establish hosting/DNS authority.

**Non-obvious claim (only):** a passing working-tree harness did not supersede the committed Round 2 blocker report until the tested site, harness, screenshots, and superseding report entered one selective commit.

**Rejected alternative (only):** authorize deployment from local browser success alone. Rejected because neither repository integration nor external hosting/DNS control follows from a local pass.

## Pareto verdicts

| Move ID | Verdict | R3 disposition |
|---|---|---|
| `XS-R1S-001_PROVENANCE` | **KEEP** | Checksum-pinned containment retained; producer authentication remains unclaimed. |
| `XS-R1S-002_COPY` | **KEEP / CLOSED** | Public copy is reconciled to README/SPEC platform and config boundaries. |
| `XS-R1S-003_METADATA` | **KEEP** | Dark `color-scheme` metadata is isolated and consistent with the page. |
| `XS-R1S-004_BROWSER` | **KEEP / VERIFIED** | Raw final browser result is PASS; screenshots, dimensions, hashes, interaction, and zero-error arrays are recorded above. |
| `XS-R1S-005_COMMIT_GATE` | **KEEP / CLOSED BY SELECTIVE COMMIT** | Tested website, harness, evidence images, and report are integrated together. |
| `XS-R1S-006_AUTHORITY_GATE` | **KEEP BLOCKED** | Hosting/DNS authority remains absent. |
| `XS-R1S-007_SUPERSEDE_REPORT` | **KEEP / CLOSED** | This R3 report supersedes project status while preserving historical reports. |

EVIDENCE_AUDIT: 7 moves | 7 reconciled | 0 spoofed | 1 explicit conflict | 1 non-obvious claim | 1 rejected alternative | screenshots 2/2 exact | deterministic checks PASS | browser console/page/network errors 0/0/0 | Rust 215/215 | `audit_hash=cffccc96b63b7cb0ecbada7c658074250a6a88402e270fc99625a18b189c1cdb`.

**Source-map protocol caveat:** source-map labels such as `repo-diff`, `browser-harness`, or role/route names identify evidence origins; they are not model-name prefixes and must not be interpreted as model identity. The fuller teammate source map remains withheld/unavailable beyond the disclosed producers above.

## Selective commit delta

Relative to parent `c2e3958`, the intended commit contains exactly 11 paths: eight `website/` changes (`index.html`, `style.css`, `main.js`, new `checks.mjs`, three optimized PNGs, and deletion of unused `assets/logo-badge.png`), the two R3 screenshots, and this R3 report. It changes no Rust, packaging, prior report, hosting, DNS, `.xbreed/`, or unrelated file. The integrated changes close the selected-slot invariant, activation semantics, live status, responsive navigation/image sizing, qualified public copy, deterministic checks, and evidence gates.

## Remaining release boundary

Round 3 establishes a reproducible committed static-site state; it does not establish production authority. Hosting and DNS remain untouched, and deployment stays blocked until independent control evidence is supplied.
