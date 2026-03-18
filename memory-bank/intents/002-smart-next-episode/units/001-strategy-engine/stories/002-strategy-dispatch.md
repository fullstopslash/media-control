---
story: 002-strategy-dispatch
unit: 001-strategy-engine
intent: 002-smart-next-episode
priority: Must
estimate: M
---

## Story: Strategy Dispatch and Integration

### Technical Story
**Description**: Wire the strategy system into mark-watched-and-next. After marking watched and finding the queue empty, resolve the library, match a rule, and dispatch to the strategy.
**Rationale**: This is the integration point that makes everything work.

### Acceptance Criteria
- [ ] `resolve_next_strategy()` function: takes library name, returns matched strategy
- [ ] Rules matched in order; first match wins
- [ ] No matching rule falls back to NextUp
- [ ] mark-watched-and-next calls: mark watched → try queue → resolve library → dispatch strategy
- [ ] Strategy errors don't prevent mark-watched from succeeding (best-effort)
- [ ] Strategy dispatch calls the appropriate strategy function (stubbed initially, implemented in unit 003)
