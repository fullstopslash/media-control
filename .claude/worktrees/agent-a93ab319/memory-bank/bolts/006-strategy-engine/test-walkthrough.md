---
stage: test
bolt: 006-strategy-engine
created: 2026-03-18T21:15:00Z
---

## Test Report: strategy engine

### Summary

- **Tests**: 177/177 passed (full suite)
- **New tests**: 6 (config parsing, resolve_strategy matching)
- **Flake check**: 10 consecutive runs, 0 flakes
- **Regressions**: 0

### New Tests

| Test | What it verifies |
|------|-----------------|
| parse_next_episode_rules | TOML deserialization of all 4 strategy types |
| resolve_strategy_matches_first_rule | First matching rule wins |
| resolve_strategy_case_insensitive | Library name matching ignores case |
| resolve_strategy_uses_default_rule | Rule with no library acts as fallback |
| resolve_strategy_no_rules_defaults_to_next_up | Empty config falls back to NextUp |
| empty_next_episode_config_defaults | Default Config has no rules |

### Acceptance Criteria Validation

- ✅ Config parses `[[next_episode.rules]]` with strategy enum
- ✅ Default config has no rules (empty vec)
- ✅ resolve_strategy matches first matching rule, falls back to NextUp
- ✅ Case-insensitive library name matching
- ✅ mark_watched_and_next uses strategy dispatch
- ✅ All existing tests pass
