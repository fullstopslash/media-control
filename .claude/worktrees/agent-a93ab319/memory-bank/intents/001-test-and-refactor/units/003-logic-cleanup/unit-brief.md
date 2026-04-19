---
unit: 003-logic-cleanup
intent: 001-test-and-refactor
phase: inception
status: ready
created: 2026-03-18T13:00:00Z
updated: 2026-03-18T13:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Logic Cleanup

## Purpose

Refactor command implementations for clarity, reduced complexity, and consistent patterns. The test suite from unit 002 provides the safety net. Full rewrites are allowed where beneficial.

## Scope

### In Scope
- Simplify `exit_fullscreen` (remove unused param, flatten retry loop)
- Restructure `avoid` command (reduce 4-case nesting, extract helpers)
- Deduplicate `close` command (merge identical killwindow branches)
- Fix semantically incorrect error variants (chapter.rs WindowNotFound)
- Remove any remaining verbose error mapping patterns
- Flatten deeply nested control flow across all commands

### Out of Scope
- Changing the public API (CLI interface, config format)
- Adding new features
- Changing behavior (all tests must continue to pass)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-4 | Logic Cleanup and Simplification | Must |
| FR-5 | Error Handling Consistency | Should |

---

## Domain Concepts

### Key Operations
| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| simplify_avoid | Restructure avoid into clearer case handling | Current avoid.rs | Cleaner avoid.rs, same behavior |
| simplify_fullscreen | Remove unused param, flatten retry | Current fullscreen.rs | Cleaner fullscreen.rs |
| dedup_close | Merge identical kill branches | Current close.rs | Simpler close.rs |
| fix_errors | Correct error variants, remove redundant mapping | All command files | Consistent error handling |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 2 |
| Should Have | 1 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-simplify-avoid | Simplify avoid command logic | Must | Planned |
| 002-simplify-fullscreen-close | Simplify fullscreen and close commands | Must | Planned |
| 003-error-consistency | Error handling consistency pass | Should | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| 001-mock-infrastructure | Need mock for running tests during refactor |
| 002-test-coverage | Tests are the safety net; must exist before refactoring |

### Depended By
None - this is the final unit.

---

## Constraints

- No behavioral changes (all existing + new tests must pass)
- No public API changes
- Prefer incremental refactors that can be verified step by step

---

## Success Criteria

### Functional
- [ ] All 118+ existing tests pass <!-- tw:d8d63948-8326-4928-a21d-eddebfbd9c65 -->
- [ ] All new E2E tests from unit 002 pass <!-- tw:f2015fb9-4e43-4ce9-92b0-b80157a07470 -->
- [ ] No behavioral regressions <!-- tw:3c1dae72-1a4e-47d6-86ed-c86576a74af3 -->

### Maintainability
- [ ] No function exceeds 4 levels of nesting <!-- tw:dc36dc3f-13ed-47a0-8375-6d821a6813fe -->
- [ ] No duplicated blocks >5 lines across commands <!-- tw:74f7ecb3-5bac-44f4-8cfa-1c4e66637040 -->
- [ ] Consistent error conversion pattern everywhere <!-- tw:10274f3f-e3dc-4ab6-b42f-22ef6363c511 -->
