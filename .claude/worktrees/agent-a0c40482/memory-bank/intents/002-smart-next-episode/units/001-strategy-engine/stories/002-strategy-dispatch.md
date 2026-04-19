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
- [ ] `resolve_next_strategy()` function: takes library name, returns matched strategy <!-- tw:72485d2b-1c81-4c08-9f9b-445e05f37f15 -->
- [ ] Rules matched in order; first match wins <!-- tw:af19d32e-5c1e-45c2-843a-b2ef0e518575 -->
- [ ] No matching rule falls back to NextUp <!-- tw:bcf16397-c855-47c2-9b6e-01140af76b9a -->
- [ ] mark-watched-and-next calls: mark watched → try queue → resolve library → dispatch strategy <!-- tw:2999f44e-8f32-4b65-9f75-8c524b239642 -->
- [ ] Strategy errors don't prevent mark-watched from succeeding (best-effort) <!-- tw:e79b45c7-a601-4985-a9e3-4fe9db06d13c -->
- [ ] Strategy dispatch calls the appropriate strategy function (stubbed initially, implemented in unit 003) <!-- tw:6f1952a1-6120-4eea-9e68-7dd44b6e2251 -->
