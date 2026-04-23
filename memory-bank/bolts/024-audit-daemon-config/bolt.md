---
id: 024-audit-daemon-config
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: planned
stories:
  - daemon-cmd-stop-wait-for-exit
  - daemon-start-lock-toctou
  - config-validate-pattern-regex-nfa-cap
created: 2026-04-23T00:00:00Z
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

## Bolt: 024-audit-daemon-config

### Objective
Tighten daemon lifecycle and config validation. Two files:
`crates/media-control-daemon/src/main.rs`, `crates/media-control-lib/src/config.rs`.

### Stories Included

- [ ] **daemon-cmd-stop-wait-for-exit** — `daemon/main.rs:755` — `cmd_stop`
  sends SIGTERM and returns. A subsequent `start` race-loses against the
  still-running daemon's PID file release. Add a poll loop after SIGTERM that
  waits up to ~2s for the PID to actually exit (re-check `is_process_running`
  every 50ms), then SIGKILL if it didn't exit in time. Return error if KILL
  also fails to take effect.

  Note: there was a prior fix in this area (`fix: cmd_stop wait-for-exit, ...`
  in 476a43a) — verify whether THIS specific code path is the same one or a
  related/missed sibling. Audit the function and any sibling stop functions.

- [ ] **daemon-start-lock-toctou** — `daemon/main.rs:636` — TOCTOU between
  `release_start_lock` and re-acquire during start-lock recovery. Restructure
  so the lock is held across the entire recovery window, or use a single
  atomic operation (e.g., `flock(LOCK_EX | LOCK_NB)` + check-pid-fresh
  inside the lock).

- [ ] **config-validate-pattern-regex-nfa-cap** — `config.rs:280` —
  `validate_pattern_regexes` uses bare `Regex::new` (no NFA size limit).
  Runtime path uses `RegexBuilder::size_limit(REGEX_NFA_SIZE_LIMIT).build()`.
  An over-large regex passes validation but is silently dropped at runtime.
  Switch validator to use `RegexBuilder` with the same cap. (`validate_override_regexes`
  already does this — copy the pattern.)

### Expected Outputs
- daemon/main.rs + config.rs touched
- Daemon lifecycle tests if practical (process spawning makes this tricky;
  acceptable to add comments documenting the race-window invariant)
- `cargo check --workspace` clean
- `cargo test --workspace` clean

### Dependencies
None.
