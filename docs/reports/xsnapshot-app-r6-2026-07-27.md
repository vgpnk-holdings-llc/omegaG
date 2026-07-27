# xsnapshot app — Round 4 cap and release audit trail (R6)

**Date:** 2026-07-27
**Round status:** maximum round reached; local frontier saturated
**Source publication:** explicitly authorized for this selective commit and normal push
**Production authorization:** `production_authorized=false`; no deployment, hosting, or DNS mutation was attempted

## Pre-round publication and source checks

Before Round 4, local `HEAD`, local `master`, its upstream, `origin/master`, and the remote `refs/heads/master` were equal at `e7af2b39a43229bc44ce285fbf1611a79009fb97`. CodeQL run `30294826337` completed successfully at that SHA and code-scanning alerts were zero. This is source-only confirmation: it verifies the published source state, not a production deployment or production reachability.

## Production labrat — HYPOTHESIS / METHOD / RESULT

**HYPOTHESIS:** The apex `xsnapshot.app` or `www.xsnapshot.app` is production-reachable after the source push.

**METHOD:** Probe both names through isolated local resolution and independent Google and Cloudflare DNS-over-HTTPS queries; query the `.app` registry through RDAP; attempt HTTPS with curl and an OpenSSL SNI/TLS handshake. Use no cookies or browser state and perform no DNS, hosting, or deployment mutation.

**RESULT:** DISPROVED AT DNS. Both apex and `www` returned NXDOMAIN (`Status: 3` from both DoH resolvers, with `.app` SOA authority); registry RDAP returned HTTP 404; local resolution failed; curl exited before HTTP with “Could not resolve host”; and OpenSSL failed name lookup before a TLS handshake. Consequently there is no TLS or HTTP endpoint to inspect.

GitHub independently reports `has_pages=false`; the Pages API returns HTTP 404; deployments are empty; and environments total zero. Production authority remains absent, so `production_authorized=false`. With no production endpoint, deployed security headers—including CSP `frame-ancestors`, `X-Content-Type-Options`, and `Referrer-Policy`—are unverifiable.

## Round-4 move verdicts

| Move ID | Verdict | Auditable disposition |
|---|---|---|
| `R4S-001_REMOTE_CODEQL_REVERIFY` | **KEEP AS CONFIRMATION ONLY** | Remote/local equality and successful CodeQL with zero alerts confirm source publication only; no product or production move. |
| `R4S-002_PRODUCTION_BLOCKER` | **KEEP BLOCKED** | NXDOMAIN, RDAP 404, absent Pages/deployments/environments, absent authority, and unverifiable headers keep production blocked. |
| `R4S-003_PRODUCTION_PROBE` | **KEEP AS EMPIRICAL CONFIRMATION** | The production labrat reproduced the DNS boundary; TLS and HTTP cannot begin. |
| `R4S-004_XBREED_IGNORE_HYGIENE` | **KEEP / SOLE NEW MOVE** | Add root-scoped `/.xbreed` to `.gitignore`; red-to-green ignore evidence and diff hygiene pass without changing product behavior. |
| `R4S-005_MAX_ROUND_STOP` | **KEEP / STOP** | The round cap is reached; unchanged production probes and duplicate source verification are saturated. |

The local frontier improved only on mailbox hygiene. All product and production probes were otherwise saturated.

## Hygiene red→green and release verification

- Before the move, the root `.gitignore` had no rule excluding the team mailbox tree; after adding `/.xbreed`, `git check-ignore -v .xbreed/mailbox/events.ndjson` resolves to that exact root-scoped rule.
- `git ls-files '.xbreed/**'` returns no tracked paths.
- Random spot-check `R4S-004_XBREED_IGNORE_HYGIENE` passed: the retained `cdx` executor independently confirmed the sole pre-report diff, `git diff --check`, exact `check-ignore` mapping, and empty tracked `.xbreed` set.
- `node website/checks.mjs` passes.
- With `TMPDIR=$PWD/target/tmp`, `cargo test --locked` passes.
- `git diff --check` passes.
- Pre-commit inspection covers status, unstaged diff, recent log, remote, and the exact staged boundary. Only `.gitignore` and this report are staged.
- Before push, `origin/master` is fetched and verified not ahead; a normal push dry-run passes. After the normal push, local `master`, its upstream, and `origin/master` are equal.

## Evidence authentication

`EVIDENCE AUDIT: 5 moves with evidence, 0 moves without, 0 dropped, 0 spoof_flagged`

The exact five-entry sorted SOURCE_MAP preimage is:

```text
[{"move_id":"R4S-001_REMOTE_CODEQL_REVERIFY","source_prefix":"cdx"},{"move_id":"R4S-002_PRODUCTION_BLOCKER","source_prefix":"cdx"},{"move_id":"R4S-003_PRODUCTION_PROBE","source_prefix":"cdx"},{"move_id":"R4S-004_XBREED_IGNORE_HYGIENE","source_prefix":"cdx"},{"move_id":"R4S-005_MAX_ROUND_STOP","source_prefix":"cdx"}]
```

SHA-256 of those exact UTF-8 bytes is `audit_hash=a4090d00e9719494b93b0eba19be763587abdc72c75c28caa219d61aafdf0306`: verified hash match.

**Blinding-protocol disclosure:** the distiller prematurely included the source map in its synthesis payload. This is a protocol defect; no source-based rescoring occurred, and the move verdicts were not changed using source identity.

**Non-obvious claim (only):** successful source security analysis does not establish that a production endpoint exists or that its response headers are configured.

**Rejected alternative (only):** continue probing unchanged production surfaces after the maximum round; rejected because the probes are saturated and the remaining blockers require external domain, DNS, and hosting authority.

## Selective commit boundary and stop

The authorized commit contains only `.gitignore` and `docs/reports/xsnapshot-app-r6-2026-07-27.md`. It excludes `.xbreed/`, product code, hosting/DNS configuration, and every unrelated path. No force push, amend, or history rewrite is used. Round 4 is the maximum-round stop.
