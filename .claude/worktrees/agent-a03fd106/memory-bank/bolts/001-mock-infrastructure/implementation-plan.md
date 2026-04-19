---
stage: plan
bolt: 001-mock-infrastructure
created: 2026-03-18T14:00:00Z
---

## Implementation Plan: mock-infrastructure

### Objective
Build a mock Hyprland IPC server and test harness so all commands can be tested end-to-end without a running Hyprland instance.

### Deliverables
- `crates/media-control-lib/src/test_helpers.rs` - Mock server + test context module (behind `#[cfg(test)]`)
- Tests for the mock infrastructure itself

### Dependencies
- `tempfile` (already a dev-dependency) for temp socket paths
- `tokio::net::UnixListener` for the mock server
- `std::sync::Arc<tokio::sync::Mutex<_>>` for shared state between mock task and test code

### Technical Approach

**Mock Server Design:**

The mock server is a tokio task that:
1. Binds a `UnixListener` to a temp file path
2. Accepts connections in a loop
3. For each connection: reads the full command (until write half is shut down), looks up the response in a `HashMap<String, String>`, writes the response, closes
4. Records each received command in a `Vec<String>` for later assertion

**Response Matching:**
- Exact match first (e.g., "j/clients")
- Prefix match for dispatch/keyword/batch (e.g., command starts with "dispatch" → return "ok")
- Default response "ok" for unmatched commands

**Key Types:**
```
MockHyprland {
    socket_path: PathBuf,
    commands: Arc<Mutex<Vec<String>>>,       // captured commands
    responses: Arc<Mutex<HashMap<String, String>>>,  // configurable responses
    _handle: JoinHandle<()>,                 // server task handle
}
```

**CommandContext for Tests:**
- Change `HyprlandClient::with_socket_path` from `#[cfg(test)]` to always available (it's a one-liner, no harm)
- Add `CommandContext::for_test(client: HyprlandClient, config: Config) -> Result<Self>`
- This compiles the window matcher from config.patterns and returns a ready context

**Test Helper Extensions:**
- `make_clients_json(clients: &[Client]) -> String` - serialize a vec of clients to JSON for mock responses
- `make_monitors_json(monitors: &[Monitor]) -> String` - same for monitors
- Reuse existing `make_client` / `make_client_full` for constructing test data

### Acceptance Criteria
- [ ] MockHyprland starts, accepts connections, returns configured responses <!-- tw:522b3163-7c1b-4d08-8dc2-1d827e4dd229 -->
- [ ] Commands are captured in order and inspectable <!-- tw:d4e7bdad-0272-4c15-b962-fb399998bfb2 -->
- [ ] HyprlandClient works with mock socket (get_clients, dispatch, batch, keyword all work) <!-- tw:71c9890e-89bf-42cc-96f3-6bfb077147c5 -->
- [ ] builds a working context with mock client <!-- tw:678227a4-6873-4577-a08e-18614214db89 -->
- [ ] Mock handles concurrent connections (batch uses a single connection, but multiple sequential commands each open a new one) <!-- tw:daf916b8-1fe2-40d1-b75f-e93317f97dbf -->
- [ ] No flaky tests from socket timing <!-- tw:b04d18b0-ac9c-4667-90e7-f96f17f2d502 -->
- [ ] Mock cleans up temp socket on drop <!-- tw:48c42f1f-ed36-4c72-93c9-c6aa2c4b20f4 -->
