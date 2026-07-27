# xsnapshot app — Round 2 corrected release and audit trail (R4)

**Date:** 2026-07-27

**Round status:** Round 2 corrections verified in the current tree; this append-only R4 report supersedes R3's source-authentication characterization without rewriting R1–R3

**Release status:** local functional site ready once this selective commit lands; production blocked externally
**Production authorization:** `production_authorized=false`; no deployment was attempted

## Round 2 axes, roster, and xask targets

| Axis / move | Roster assignment | xask target | Final target / disposition |
|---|---|---|---|
| `R2S-001_PROVENANCE_SCOPE` — artifact provenance and containment | Reverse-engineering/provenance verifier | None | Record the locally available, checksum-pinned, safe eight-entry ZIP without asserting producer provenance. |
| `R2S-002_CONTROLLER_COPY_GUARD` — controller-copy correctness | Public-copy correctness researcher | None | Correct the DS4/DualSense cursor and touchpad claims and retain deterministic red→green guards. |
| `R2S-003_BROWSER_STATE` — responsive interaction | Browser auditor/interaction reviewer | None | Verify the final current tree at exact desktop/mobile viewports with zero browser error arrays. |
| `R2S-004_STATIC_RUST_BOUNDARY` — static/Rust regression | Regression verifier | None | Preserve non-website code and pass all 215 locked tests. |
| `R2S-005_SECURITY_HEADER_GATE` — deployment security | Security reviewer | None | Keep production blocked until an authorized host applies and verifies required headers. |
| `R2S-006_HOSTING_DNS_AUTHORITY` — release control | Release-control reviewer | None | Distinguish repository administration from hosting and DNS authority; do not deploy. |
| `R2S-007_R1_AUDIT_SUPERSEDE` — evidence authentication | Corrected evidence distiller | None | Retain reproducible R2 attestation and reject unsupported canonical status for the historical R1/R3 hash. |
| Evidence/release trail | `ccs-scribe-r2` | None | Append this report, preserve R1–R3, and selectively commit only the authorized five paths. |

There was no xask gate on any axis. Evidence for this evidence/release-trail axis is none; it is documentation of supplied and independently rerun checks.

## Final controller copy correction and red→green guards

The final two-file correction replaces the ambiguous claim that “touchpad swipe or left stick” moves the cursor. The site now states that DualSense touchpad swipe moves the cursor, while DualShock 4 uses left-stick mouse because its touchpad coordinates are unsupported. It separately states that touchpad press clicks on both controllers while touchpad handling is enabled. The fixed-map labels now read `Left stick / DualSense touchpad swipe — mouse cursor` and `Touchpad press — left-click on DualSense and DS4`.

`website/checks.mjs` adds three regression assertions for the corrected specification sentence and both map labels. These guards failed against the prior copy and pass against the corrected copy: red→green.

## Browser evidence — HYPOTHESIS / METHOD / RESULT

**HYPOTHESIS:** The final current-tree site presents the corrected DualSense/DS4 sentence and map labels at 1440×900 and 390×844, preserves selected slot 3 while keyboard activation updates status, resolves its anchors/assets without overflow, and emits no console, page, or network errors.

**METHOD:** Serve the current `website/` tree over loopback HTTP and use Chromium 150 with a fresh temporary profile, headless and GPU-disabled, at desktop 1440×900 and mobile-emulated 390×844. At both viewports inspect the corrected sentence and labels, horizontal overflow, keyboard Tab+Enter activation, selected/live state, `#lightbar`, `#layer`, and `#specs`, loaded images, resource responses, and console/page/network error collections; capture final PNGs and rerun `node website/checks.mjs`.

**RESULT:** PASS. The exact corrected sentence and map labels were present and visible at both viewports; no horizontal overflow occurred; keyboard Tab+Enter changed the pressed/live state to `complete-unread` while slot 3 remained selected; all three anchors worked; all loaded images completed and browser resources returned 200. Console/page/network errors were exactly **0/0/0**. `node website/checks.mjs` passed.

| Final current-tree artifact | Exact dimensions | SHA-256 |
|---|---:|---|
| `docs/reports/assets/xsnapshot-app-r4-desktop.png` | 1440×900 | `307725d4641b6a444aa8029a610c9275c1d7f8d4d2ec5658fa6af572df28734b` |
| `docs/reports/assets/xsnapshot-app-r4-mobile.png` | 390×844 | `87e3637b1b53df1ad912af757be822ba5eb413dd518128647474ece9126f1a70` |

## Archive availability, integrity, and safe entries

The supplied external archive is not tracked in or located beneath the omegaG checkout. Its SHA-256 is `1d9e3c7ba894d4328b4ce4f6fc85b60a97a0f2cb85b3f810f018bd3e3afdf6d1`; `unzip -t` passes. The exact eight entries are:

1. `app/index.html`
2. `app/main.js`
3. `app/style.css`
4. `app/assets/controller-mark.png`
5. `app/assets/favicon.ico`
6. `app/assets/hero-journey.png`
7. `app/assets/logo-badge.png`
8. `app/assets/masterpiece.png`

All eight are beneath `app/`; checks found zero absolute paths, traversal components, symlinks, duplicates, or case-fold collisions. This establishes local artifact availability and integrity, not authenticated producer provenance.

## Hosting, DNS, and security gate

- DNS-over-HTTPS and local resolver probes returned NXDOMAIN for both `xsnapshot.app` and `www.xsnapshot.app`.
- The `.app` registry RDAP lookup returned HTTP 404.
- GitHub Pages returned HTTP 404 and the repository reports `has_pages=false`.
- GitHub API queries returned no deployments and zero environments.
- No Vercel, Netlify, Cloudflare Wrangler, or Firebase provider configuration was found in the repository; their CLIs were absent.
- Repository permission reports `admin=true`, but repository administration does not establish domain registration, DNS, or production-host authority.

Deployment security headers are not defined or evidenced. Production remains blocked until an authorized host applies and verifies a CSP including `frame-ancestors`, `X-Content-Type-Options`, and `Referrer-Policy`. Therefore `production_authorized=false`; no hosting/DNS mutation and no deployment attempt occurred.

## Reproducible evidence authentication

The original blinded Round 2 attestation is `audit_hash=a25ad649f50eb1bee0e14f683581b56942bffac39301f91704b1808efd4de86b`.

The exact sorted SOURCE_MAP preimage is:

```text
[{"move_id":"R2S-001_PROVENANCE_SCOPE","source_prefix":"cdx"},{"move_id":"R2S-002_CONTROLLER_COPY_GUARD","source_prefix":"cdx"},{"move_id":"R2S-003_BROWSER_STATE","source_prefix":"cdx"},{"move_id":"R2S-004_STATIC_RUST_BOUNDARY","source_prefix":"cdx"},{"move_id":"R2S-005_SECURITY_HEADER_GATE","source_prefix":"cdx"},{"move_id":"R2S-006_HOSTING_DNS_AUTHORITY","source_prefix":"cdx"},{"move_id":"R2S-007_R1_AUDIT_SUPERSEDE","source_prefix":"cdx"}]
```

SHA-256 of those exact UTF-8 bytes is `a25ad649f50eb1bee0e14f683581b56942bffac39301f91704b1808efd4de86b`: verified hash match. Random spot-check `R2S-005` passed against the retained security-review claim.

`EVIDENCE AUDIT: 7 moves with evidence, 0 moves without, 0 dropped, 1 spoof_flagged`

## Reviewer corrections and historical supersession

- **`R2S-001` revised:** any broad archive-absence characterization is partly contradicted. The supplied external archive exists outside the checkout and passes hash, integrity, and containment checks. R3's line 29 remains accurate only in its narrow statement that the ZIP is not present in the checkout.
- **`R2S-007` rejected:** Round 1 records `4e2ca00d0779614175396e0411300671001a2a27161db4ceb9ec4a28959d0e10`, and R3 records `cffccc96b63b7cb0ecbada7c658074250a6a88402e270fc99625a18b189c1cdb`, as historical synthesis claims. The historical Round-1/R3 hash lacks a retained valid SOURCE_MAP and exact preimage, so it is unverified, not independently reproducible, and not canonical. R4 explicitly supersedes R3's source-authentication characterization while preserving R1–R3 unchanged. This is an attestation/schema defect, not evidence of content fraud.

## Verification gates

- `git diff --check` → pass, exit 0.
- `node website/checks.mjs` → `website checks: pass`.
- `cargo test --locked` → **215/215 passed**, 0 failed; the three existing dead-code warnings remain.
- Loopback HTTP asset/anchor checks → six local resources returned 200 and `#lightbar`, `#layer`, and `#specs` resolved.
- Screenshot dimension/hash checks → both exact dimensions and both SHA-256 values reproduced as recorded above.
- Pre-commit inspection covered `git status`, unstaged diff, and the latest ten commits. Selective staging excludes `.xbreed/` and all unrelated paths.

## Final Pareto verdicts and frontier saturation

| Move ID | Final verdict | Disposition |
|---|---|---|
| `R2S-001_PROVENANCE_SCOPE` | **KEEP / REVISED** | Preserve the archive hash, safe-entry audit, and unauthenticated-producer boundary without recording a workstation path. |
| `R2S-002_CONTROLLER_COPY_GUARD` | **KEEP / CLOSED** | Corrected DS4/DualSense claims and deterministic guards are ready to integrate. |
| `R2S-003_BROWSER_STATE` | **KEEP / VERIFIED** | Final desktop/mobile interaction and 0/0/0 browser evidence pass. |
| `R2S-004_STATIC_RUST_BOUNDARY` | **KEEP / VERIFIED** | Static scope is preserved and Rust remains 215/215. |
| `R2S-005_SECURITY_HEADER_GATE` | **KEEP BLOCKED** | Header verification requires an authorized production host. |
| `R2S-006_HOSTING_DNS_AUTHORITY` | **KEEP BLOCKED** | DNS/hosting authority remains externally absent. |
| `R2S-007_R1_AUDIT_SUPERSEDE` | **REJECT HISTORICAL CANONICAL STATUS / KEEP R2 REPRODUCIBLE ATTESTATION** | R2 SOURCE_MAP hash reproduces; the historical R1/R3 claim does not. |

**Frontier saturation:** all local no-harm moves are closed: truthful copy, red→green guards, final browser artifacts, deterministic/static checks, Rust preservation, and reproducible R2 evidence authentication. The local functional site is ready once this commit lands. The remaining frontier is external: production is blocked on independently established DNS/hosting authority and verified security headers.

**Non-obvious claim (only):** repository `admin=true` is authority over repository settings, not proof of registrant, DNS-zone, or production-host control.

**Rejected alternative (only):** deploy from repository administration and local success alone. Rejected because DNS/hosting authority and production security-header evidence are absent.

## Selective commit boundary

The authorized commit stages only `website/index.html`, `website/checks.mjs`, this R4 report, and the two R4 screenshots. It excludes `.xbreed/`, R1–R3 historical reports, Rust, packaging, hosting, DNS, and every unrelated path. No push is performed.
