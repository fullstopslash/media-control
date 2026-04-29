---
id: 020-audit-jellyfin-hardening
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: complete
stories:
  - jellyfin-sort-params-validation
  - jellyfin-request-empty-body-drain
  - jellyfin-credential-error-context
created: 2026-04-23T00:00:00Z
completed: 2026-04-23T22:25:28Z
status_backfilled: 2026-04-29T12:00:00Z
source_commit: 3844dd08
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 020-audit-jellyfin-hardening

### Objective
Harden the Jellyfin HTTP client: validate query parameters, reclaim connection
slots from response bodies, preserve file context in credential errors. All
edits in `crates/media-control-lib/src/jellyfin.rs`.

### Stories Included

- [ ] **jellyfin-sort-params-validation** — `sort_by` / `sort_order` strings
  flow into URL query params unvalidated (jellyfin.rs:1201 area). Constrain
  via enums (`SortBy::DateCreated`, `SortOrder::{Asc,Desc}`), serialize via
  `Display`. Reject unknown values at the type boundary, not server-side.

- [ ] **jellyfin-request-empty-body-drain** — `request_empty` (and any
  `post_*_empty` variants) discards the body without draining, which prevents
  the connection from returning to the pool and starves subsequent requests.
  Drain to EOF (e.g., `let _ = response.bytes().await;`) before discarding.
  Search for ALL call sites; the prior body-drain fix missed this one.

- [ ] **jellyfin-credential-error-context** — `load_credentials` returns parse
  errors stripped of the file path (jellyfin.rs:511 area). Wrap parse errors
  to include the `path: PathBuf` so users see "failed to parse
  /home/.../cred.json: ..." rather than a bare TOML error.

### Expected Outputs
- jellyfin.rs only
- Sort enums with `Display` impls + serde `Serialize` if needed
- All `request_empty`-style helpers consume the body before drop
- Credential error variant carries path
- `cargo check --workspace` clean
- `cargo test --workspace` clean

### Dependencies
None.

### Completion (status backfilled 2026-04-29)

Frontmatter sync — work shipped in commit `3844dd08` (2026-04-23). Verified
2026-04-29 against the live tree:

- `jellyfin-sort-params-validation` ✅ — `pub enum SortBy` at
  `jellyfin.rs:399`; `pub enum SortOrder` at `jellyfin.rs:443`
- `jellyfin-request-empty-body-drain` ✅ — `let _ = response.bytes().await;`
  at `jellyfin.rs:864`
- `jellyfin-credential-error-context` ✅ — new `CredentialsParseAt {
  path: PathBuf, source: serde_json::Error }` variant at `jellyfin.rs:43-50`
  with formatter that displays the path
