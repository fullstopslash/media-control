# Hyprland IPC Analysis & Migration Plan

## Executive Summary

**Key Finding:** `hyprwire` and `hyprtools` are C/C++ libraries without Rust bindings. The relevant option for Rust is the `hyprland-rs` crate.

**Recommendation:** Consider migrating to `hyprland-rs` for better maintainability, but current custom implementation is performant and has no critical issues.

---

## Current Implementation Analysis

### Technology Stack
- **Direct socket access:** `tokio::net::UnixStream`
- **Protocol:** Manual implementation of Hyprland IPC protocol
- **Location:** `crates/media-control-lib/src/hyprland.rs` (606 lines)

### What We Have
```rust
// Direct Unix socket communication
pub struct HyprlandClient {
    socket_path: PathBuf,
}

// Methods:
- command()      // Low-level socket I/O
- dispatch()     // Dispatcher commands
- batch()        // Batched commands ([[BATCH]])
- get_clients()  // Query j/clients
- get_monitors() // Query j/monitors
- get_active_window()
- keyword()      // Set config options
```

### Daemon Implementation
- **File:** `crates/media-control-daemon/src/main.rs`
- **Socket2 usage:** Event streaming from `.socket2.sock`
- **Event handling:** Direct line-by-line parsing of events
- **Events monitored:** workspace, activewindow, movewindow, openwindow, closewindow, swapwindow
- **Debouncing:** 15ms (configurable) to batch rapid events

---

## Research Findings

### 1. hyprwire (C/C++ Library)

**What it is:**
- IPC library introduced in Hyprland 0.53.0 (Dec 2025)
- Improves hyprctl communication
- Used by hyprpaper 0.8.0+

**Rust availability:**
- ❌ No Rust crate available
- ❌ No official Rust bindings
- Native C/C++ only

**Verdict:** Not applicable for Rust projects.

### 2. hyprtools / hyprtoolkit

**What it is:**
- Pure C++ GUI toolkit for Wayland applications
- Part of Hypr ecosystem
- For building native Hyprland applications

**Rust availability:**
- ❌ No Rust bindings
- C++ only

**Verdict:** Not relevant for IPC communication; GUI toolkit only.

### 3. hyprland-rs (Rust Crate)

**What it is:**
- Unofficial Rust wrapper for Hyprland's IPC
- Community-maintained by @yavko and contributors
- GitHub: [hyprland-community/hyprland-rs](https://github.com/hyprland-community/hyprland-rs)

**Status:**
- ✅ Actively maintained (seeking v0.4 contributors)
- ✅ Latest stable: v0.3.13 (Feb 2024)
- ✅ Beta available: v0.4.0-beta.3
- ✅ 374 stars, 284 dependent projects
- ✅ 50 contributors

**Features:**
- 6 main modules:
  1. `data` - Get compositor info
  2. `event_listener` - Monitor events (socket2)
  3. `dispatch` - Call dispatcher functions
  4. `keyword` - Set config options
  5. `config::binds` - Keybinding management
  6. `ctl` - Execute hyprctl commands
- Async support: tokio, async-std, async-net
- JSON serialization built-in
- Macros: `bind!`, `dispatch!`, `command!`
- 99.25% documentation coverage

---

## Comparison Matrix

| Feature | Current Implementation | hyprland-rs |
|---------|------------------------|-------------|
| **Lines of code** | ~606 lines | Abstracted (dependency) |
| **Maintenance** | Our responsibility | Community-maintained |
| **Type safety** | Manual deserialiation | Built-in types |
| **Event listening** | Manual parsing | `EventListener` struct |
| **Documentation** | Good (comments) | Excellent (99.25%) |
| **Testing** | Unit tests present | Well-tested (284 users) |
| **Flexibility** | Full control | Higher-level abstractions |
| **Performance** | Direct I/O (fast) | Abstraction overhead (minimal) |
| **Socket2 events** | Manual line parsing | Built-in event types |
| **Error handling** | Custom `HyprlandError` | Built-in error types |
| **Batch commands** | Implemented | Likely available |
| **Dependencies** | tokio, serde | + hyprland ecosystem |

---

## Trade-offs Analysis

### Keeping Current Implementation

**Pros:**
- ✅ Zero abstraction overhead
- ✅ Full control over protocol details
- ✅ No external dependency risk
- ✅ Already working and tested
- ✅ Minimal dependencies
- ✅ Custom optimizations possible
- ✅ Complete understanding of internals

**Cons:**
- ❌ Maintenance burden on updates
- ❌ Must track Hyprland protocol changes
- ❌ Manual type definitions for new fields
- ❌ Reinventing wheel (606 lines)

### Migrating to hyprland-rs

**Pros:**
- ✅ Community-maintained updates
- ✅ Automatic protocol compatibility
- ✅ Extensive type definitions
- ✅ Better event abstractions
- ✅ Used by 284+ projects (battle-tested)
- ✅ Excellent documentation
- ✅ Macros for cleaner code
- ✅ Less code to maintain

**Cons:**
- ❌ Dependency on external project
- ❌ Beta version (0.4) in development
- ❌ Potential API changes
- ❌ Abstraction overhead (likely negligible)
- ❌ Must track breaking changes in hyprland-rs
- ❌ Less control over low-level details

---

## Migration Path (If Chosen)

### Phase 1: Evaluate (Low Risk)
1. Add `hyprland = "0.3.13"` to Cargo.toml
2. Create proof-of-concept branch
3. Implement `get_clients()` using hyprland-rs
4. Benchmark performance difference
5. Compare code complexity

### Phase 2: Gradual Migration (Medium Risk)
1. Create compatibility layer (facade pattern)
2. Implement `HyprlandClient` wrapper over hyprland-rs
3. Keep existing tests passing
4. Migrate command by command
5. Update daemon event handling to use `EventListener`

### Phase 3: Full Migration (High Risk)
1. Replace `hyprland.rs` entirely
2. Update all imports across codebase
3. Remove custom types in favor of hyprland-rs types
4. Simplify daemon event parsing
5. Remove tokio UnixStream code

### Phase 4: Optimization
1. Use hyprland-rs macros (`dispatch!`, etc.)
2. Leverage built-in event filtering
3. Explore async runtime optimizations
4. Consider 0.4.0-beta features

---

## Recommendation

### Short Term (Now)
**Keep current implementation** for these reasons:
1. It works reliably
2. Performance is excellent (direct socket I/O)
3. Well-tested and understood
4. No urgent need for change
5. Hyprland 0.53+ doesn't require protocol changes

### Medium Term (3-6 months)
**Monitor hyprland-rs 0.4 release:**
1. Watch for v0.4 stable release
2. Review breaking changes
3. Assess community adoption
4. Evaluate new features

### Long Term (6-12 months)
**Consider migration if:**
1. Hyprland protocol changes significantly
2. hyprland-rs 0.4+ adds compelling features
3. Maintenance burden increases
4. Community adoption solidifies (500+ dependents)
5. Need for additional Hyprland features (bindings, etc.)

---

## Action Items

### Immediate
- [x] Document current implementation <!-- tw:f6a03205-dcee-4d6c-802b-b9ec5fc075e5 -->
- [x] Research hyprwire/hyprtools availability <!-- tw:78b4a798-9886-4574-9b47-5044c659b108 -->
- [x] Evaluate hyprland-rs as alternative <!-- tw:b493c5d6-42d4-4a09-a8f8-46a8035d4e3c -->
- [ ] Bookmark hyprland-rs GitHub for updates <!-- tw:aedc249f-aa30-443e-8ae0-ba1483016963 -->
- [ ] Subscribe to Hyprland release notes <!-- tw:3aceaa2f-f291-4b7b-9459-944ecd457809 -->

### Future Considerations
- [ ] Create spike branch with hyprland-rs <!-- tw:4bcb70e6-f0df-4795-b687-8931a3e2fbbc -->
- [ ] Benchmark performance comparison <!-- tw:2f566d9d-ed43-4911-bf41-6144607b9fbd -->
- [ ] Assess code reduction potential <!-- tw:5d57b57c-42c0-4745-9915-b77ee6f0fde2 -->
- [ ] Monitor hyprland-rs 0.4 stable release <!-- tw:a69fd760-fb2c-422d-a482-ec9bb4d2c58f -->
- [ ] Re-evaluate decision in 6 months <!-- tw:6f21e87e-176f-454b-b8f7-58eaedeac83e -->

---

## Sources

- [Hyprland 0.53.0 Release](https://dev.to/ashbuk/hyprland-0530-for-fedora-1afm)
- [hyprland-rs GitHub](https://github.com/hyprland-community/hyprland-rs)
- [hyprland-rs Documentation](https://docs.rs/hyprland/latest/hyprland/)
- [hyprland-rs on crates.io](https://crates.io/crates/hyprland)
- [Hyprland Wiki - IPC](https://wiki.hypr.land/IPC/)
- [Hyprland Wiki - hyprtoolkit](https://wiki.hypr.land/Hypr-Ecosystem/hyprtoolkit/)
- [Hypr Ecosystem](https://wiki.hypr.land/Hypr-Ecosystem/)

---

## Conclusion

**hyprwire** and **hyprtools** are C/C++ libraries without Rust support. The Rust ecosystem's answer is **hyprland-rs**, which is well-maintained and feature-rich. However, your current custom implementation is performant, well-tested, and requires no immediate changes.

**Recommendation:** Maintain current implementation, monitor hyprland-rs v0.4, and revisit this decision in 6 months or when Hyprland protocol changes necessitate updates.
