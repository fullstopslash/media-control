---
id: 004-stale-socket-retry
unit: 001-ipc-hardening
intent: 004-ipc-reliability
status: complete
priority: should
created: 2026-03-19T12:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 004-stale-socket-retry

## User Story

**As a** media-control user
**I want** commands to retry once after a brief wait when all socket paths fail
**So that** commands sent during mpv respawn still go through

## Acceptance Criteria

- [ ] **Given** all socket paths fail on first attempt, **When** retry is triggered, **Then** it waits 100ms and tries all paths again <!-- tw:cf75b7f4-9b0a-4a07-9af5-5d80310c09b0 -->
- [ ] **Given** mpv respawns within the retry window, **When** the retry attempt runs, **Then** the command succeeds <!-- tw:4fdcd57e-4827-41a3-9a31-e679032bcf20 -->
- [ ] **Given** all paths fail on both attempts, **When** retry is exhausted, **Then** it returns an error (no infinite loop) <!-- tw:ed0995f8-f606-400f-a256-d22823ee93e9 -->
- [ ] **Given** the first path succeeds, **When** send_mpv_script_message runs, **Then** no retry is attempted (happy path unchanged) <!-- tw:02a36da3-6f33-44a1-b3d6-2a6545761de8 -->

## Technical Notes

- Retry logic wraps the entire path iteration loop
- `tokio::time::sleep(Duration::from_millis(100))` between attempts
- Max 1 retry (2 total attempts)
- mpv respawn typically takes 100-500ms based on logs

## Dependencies

### Requires
- 001-socket-validation (retry uses same validation)
- 002-connection-timeout (retry uses same timeout logic)

### Enables
- 005-error-feedback (final error after retry exhaustion is reported)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Socket appears between attempts | Retry finds and uses it |
| mpv respawn takes > 100ms | Retry may still fail; acceptable |
| Multiple mpv instances | Tries all paths in order each attempt |

## Out of Scope

- Configurable retry count or delay
- Exponential backoff
