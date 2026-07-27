# xsnapshot app — Round 3 audited native-hero source release (R5)

**Date:** 2026-07-27
**Round status:** Round 3 local corrections and recovered publication gates verified
**Source publication:** explicitly authorized by the user for selective commit and normal push
**Production deployment:** `production_authorized=false`; source publication is not production deployment

## Scope and release boundary

This release replaces the supplied branded hero with an omegaG-native HTML/CSS composition using the existing controller mark, repoints social metadata, and deletes `website/assets/hero-journey.png`. It also adds deterministic absence/reference guards and sanitizes the current R4 report so the current tip no longer records the non-secret workstation archive path.

The current tip therefore removes both the non-secret absolute path and the supplied branded asset. Their existence in the currently unpushed ancestor commits—which this authorized source push will also publish—is retained as a known privacy/provenance caveat: no amend, history rewrite, or force-push was authorized. Audits found no credentials, secrets, cookies, or browser-state artifacts; this report does not claim that any such material existed.

The authorized selective boundary is the five tracked corrective paths—`docs/reports/xsnapshot-app-r4-2026-07-27.md`, `website/assets/hero-journey.png` (deleted), `website/checks.mjs`, `website/index.html`, and `website/style.css`—plus this R5 report and its two R5 PNGs. `.xbreed/` and every unrelated path are excluded.

## Native-hero browser evidence — HYPOTHESIS / METHOD / RESULT

**HYPOTHESIS:** The corrected current-tree site renders its omegaG-native hero at desktop 1440×900 and mobile 390×844, contains no branded hero reference, preserves its interaction and responsive behavior, resolves local assets and anchors, and emits no console, page, or network errors.

**METHOD:** Serve `website/` over loopback HTTP; inspect the settled current tree in Chromium at the two exact viewports; verify the native hero and controller mark, selected slot/state interaction, horizontal containment, local resources, `#lightbar`, `#layer`, and `#specs`; collect console/page/network error arrays; capture PNG bytes; then reproduce dimensions and SHA-256 values independently.

**RESULT:** PASS. The native hero correction is visible in both final captures, the deleted branded asset is neither referenced nor requested, local asset/anchor checks pass, and browser console/page/network errors are exactly **0/0/0**.

| Final audited artifact | Exact dimensions | SHA-256 |
|---|---:|---|
| `docs/reports/assets/xsnapshot-app-r5-desktop.png` | 1440×900 | `f361e32b5e9a2f4a07118cf838ba100153677abc6c014d2787329ab845d9287e` |
| `docs/reports/assets/xsnapshot-app-r5-mobile.png` | 390×844 | `671ba0a30179dfa8874905ae233bd28cfc5eff0dd52a8d441bd42f92d75696fe` |

## Evidence authentication and spoof exclusion

The exact evidence audit line is:

`EVIDENCE AUDIT: 0 moves with evidence, 6 moves without, 0 dropped, 1 spoof_flagged`

Evidence is none—synthesis/documentation. The spoof count is one unauthorized source family: mailbox events named `higher-orch-gemma-*` were excluded and none of their claims are used here. No count of authorized mailbox events is asserted.

The Round-3 attestation is `audit_hash=584217eb555114e7208172922e1fb40d9fc95aedead2157bdc13490d50d7b57e`. The exact six-entry sorted SOURCE_MAP preimage, with every entry mapped to `cdx`, is:

```text
[{"move_id":"R3S-001_NATIVE_HERO","source_prefix":"cdx"},{"move_id":"R3S-002_PATH_SANITIZATION","source_prefix":"cdx"},{"move_id":"R3S-003_BROWSER_PNG_PROOF","source_prefix":"cdx"},{"move_id":"R3S-004_PREPUSH_GATES","source_prefix":"cdx"},{"move_id":"R3S-005_SOURCE_VS_PRODUCTION_AUTH","source_prefix":"cdx"},{"move_id":"R3S-006_DEPLOYMENT_BLOCKER","source_prefix":"cdx"}]
```

SHA-256 of those exact UTF-8 bytes is `584217eb555114e7208172922e1fb40d9fc95aedead2157bdc13490d50d7b57e`: **verified hash match**. Random protocol spot-check `R3S-003_BROWSER_PNG_PROOF` passed: the retained `cdx` browser-evidence producer independently matched both exact PNG dimensions and hashes; its functional source-map label is materially accurate.

## Round-3 move verdicts

| Move ID | Verdict | Publication disposition |
|---|---|---|
| `R3S-001_NATIVE_HERO` | **KEEP** | Publish the native HTML/CSS/controller-mark hero, corrected social image, branded-asset deletion, and deterministic guards. |
| `R3S-002_PATH_SANITIZATION` | **KEEP WITH ANCESTRY CAVEAT** | Publish the sanitized current report while explicitly retaining the known unpushed-ancestor caveat; do not rewrite history. |
| `R3S-003_BROWSER_PNG_PROOF` | **KEEP LATER PROOF** | Publish the final native-hero captures and exact hashes; older R4 captures are not proof of this correction. |
| `R3S-004_PREPUSH_GATES` | **KEEP** | Require clean diff, deterministic, Rust, HTTP/link, PNG, secret-scan, cached-diff, and remote-safety gates. |
| `R3S-005_SOURCE_VS_PRODUCTION_AUTH` | **KEEP / SOURCE AUTHORIZED, PRODUCTION FALSE** | The user explicitly authorized this selective commit and normal push; that authorization does not authorize deployment. |
| `R3S-006_DEPLOYMENT_BLOCKER` | **KEEP BLOCKED** | Production remains blocked by absent hosting/domain/DNS authority and unverified security headers. |

## Verification and publication gates

- `git diff --check` passes on the current diff.
- `node website/checks.mjs` prints `website checks: pass`.
- The audited Rust baseline is **215/215**, 0 failed, with three existing dead-code warnings. An initial fresh run without an explicit project-local `TMPDIR` produced **209 passed, 6 failed** when temporary-file creation returned OS error 122, `Disk quota exceeded`; this was an environmental failure, not a waived gate. With `TMPDIR=/home/vhpnk/Projects/omegaG/target/tmp`, the targeted tmux test passed **1/1**. One intervening full run exposed the same tmux test as transient (**214 passed, 1 failed**); an immediate targeted rerun passed **1/1**, and the subsequent full `cargo test --locked` passed **215/215**, 0 failed, with only the three existing warnings. `target/tmp` is ignored and excluded from publication.
- Loopback HTTP checks return 200 for the current local resources; local references and `#lightbar`, `#layer`, and `#specs` resolve.
- Screenshot checks reproduce both exact dimensions and hashes above.
- Current-tree/history secret scanning finds no credentials or secrets; no cookies or browser-state artifacts were found.
- Before commit, status, unstaged diff, recent log, remote, and the exact cached name-status/diff are inspected; only the eight authorized paths are staged.
- Before push, `origin/master` is fetched and verified not ahead of local `master`; a normal push dry-run must pass. After the normal push, local `master`, its upstream, and `origin/master` must be equal.

## Deployment blocker

`xsnapshot.app` and `www.xsnapshot.app` are NXDOMAIN. GitHub Pages is absent (`has_pages=false`), and no hosting target/provider configuration or authority is established. Repository administration does not establish domain, DNS-zone, or production-host authority. No production security headers are evidenced: CSP including `frame-ancestors`, `X-Content-Type-Options`, and `Referrer-Policy` remain unverified. Consequently `production_authorized=false`; this source publication performs no production deployment, hosting mutation, or DNS mutation.

**Non-obvious claim (only):** deleting or redacting a path at the current tip does not remove its prior blob or text from Git ancestry.

**Rejected alternative (only):** treat an authorized source push as production-deployment authority; rejected because DNS, hosting authority, and deployed security-header evidence remain absent.
