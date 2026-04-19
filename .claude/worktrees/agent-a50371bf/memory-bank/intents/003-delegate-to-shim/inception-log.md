---
intent: 003-delegate-to-shim
created: 2026-03-18T22:00:00Z
completed: 2026-03-18T22:00:00Z
status: complete
---

# Inception Log: delegate-to-shim

## Overview

**Intent**: Replace mark-watched-and-next with thin mpv IPC call, remove redundant strategy code
**Type**: refactoring
**Created**: 2026-03-18

## Decision Log

| Date | Decision | Rationale | Approved |
|------|----------|-----------|----------|
| 2026-03-18 | Delegate to shim via keypress ctrl+n IPC | Shim handles strategies natively now, faster, no round-trip | Yes |
| 2026-03-18 | Keep mark-watched and mark-watched-and-stop in Rust | No shim equivalent for mark-only or mark+stop | Yes |
| 2026-03-18 | Clean delete, no feature gate | Code lives in shim fork now, no reason to keep | Yes |
