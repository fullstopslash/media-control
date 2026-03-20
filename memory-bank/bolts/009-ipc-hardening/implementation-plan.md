---
stage: plan
bolt: 009-ipc-hardening
created: 2026-03-19T12:00:00Z
---

## Implementation Plan: ipc-hardening

### Objective

Rewrite `send_mpv_script_message()` to validate sockets, timeout on connect/write, read responses, retry on failure, and propagate errors with desktop notifications.

### Deliverables

- Hardened `send_mpv_script_message()` with socket validation, timeouts, response reading, and retry
- New `MpvIpcError` variant in error.rs for structured IPC error reporting
- Error propagation through mark_watched.rs callers (remove `let _ =` swallowing)
- notify-send integration in main.rs for user-visible error feedback

### Dependencies

- `std::os::unix::fs::FileTypeExt` — socket type checking (stdlib, no new crate)
- `tokio::time::timeout` — already available via tokio
- `tokio::io::AsyncBufReadExt` — for reading response lines (already available)

No new crate dependencies needed.

### Technical Approach

#### 1. Socket validation (FR-1)
In the socket iteration loop, before `UnixStream::connect()`:
- `std::fs::metadata(path)` to stat the path
- `metadata.file_type().is_socket()` via `FileTypeExt` to check type
- Skip non-sockets with `eprintln!` warning
- Non-existent paths: already handled by `!path.exists()` check

#### 2. Connection timeout (FR-2)
Wrap the connect+write block in `tokio::time::timeout(Duration::from_millis(500), ...)`:
```
timeout(Duration::from_millis(500), async {
    let mut stream = UnixStream::connect(path).await?;
    stream.write_all(payload.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(stream)  // return stream for response reading
})
```
On `Elapsed`, log which path timed out and continue to next.

#### 3. Response verification (FR-4)
After successful write, read one line with 200ms timeout:
```
let reader = BufReader::new(&mut stream);
match timeout(Duration::from_millis(200), reader.read_line(&mut buf)).await {
    Ok(Ok(_)) => parse response JSON, warn on error field != "success"
    Ok(Err(_)) => warn, treat as success (fire-and-forget fallback)
    Err(_) => warn "no response within 200ms", treat as success
}
```
Response reading failure is a warning, not an error — the command was sent.

#### 4. Retry logic (FR-5)
Wrap the entire socket iteration in a retry loop (max 2 attempts):
```
for attempt in 0..2 {
    for socket_path in paths {
        // validate, connect, write, read response
        // on success, return Ok
    }
    if attempt == 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
return Err(...)
```

#### 5. Error feedback (FR-3)

**error.rs**: Add `MpvIpc` variant to `MediaControlError`:
```rust
#[error("mpv IPC error: {kind}")]
MpvIpc {
    kind: MpvIpcErrorKind,
    message: String,
}
```
With `MpvIpcErrorKind`: `NoSocket`, `Timeout`, `ConnectionFailed`, `ResponseError`.

**mark_watched.rs**: Remove `let _ =` on line 60 of `mark_watched_and_stop()`. All callers already propagate with `?` except this one.

**main.rs**: Add error handling after the match block:
```rust
if let Err(e) = result {
    eprintln!("media-control: {e}");
    // Fire-and-forget notify-send
    let _ = std::process::Command::new("notify-send")
        .args(["-u", "critical", "media-control", &format!("{e}")])
        .spawn();
    std::process::exit(1);
}
```
Restructure main to capture the result from the command match, then handle error uniformly.

### Acceptance Criteria

- [ ] Regular files at socket paths are skipped with stderr warning <!-- tw:5a91390e-a86a-4f8d-b74b-f99ab2a4ce30 -->
- [ ] Dead sockets timeout within 500ms per path <!-- tw:b215c65b-9fe0-4a51-b5a3-fbc3ba6e818c -->
- [ ] mpv IPC JSON response is read (200ms timeout, warn-only on failure) <!-- tw:622a78a8-1469-4417-88d1-25f52d5ae0b2 -->
- [ ] Failed first attempt retries after 100ms <!-- tw:247f931a-f1e3-4aab-9c3d-2dbb22bf9141 -->
- [ ] All IPC errors propagated to main, printed to stderr, shown via notify-send <!-- tw:720fcc0d-6c50-4659-9922-186e7a2f7418 -->
- [ ] `mark_watched_and_stop` no longer swallows errors with `let _ =` <!-- tw:88f3a096-b0ec-4f1d-8f60-64f1abeff49c -->
- [ ] Exit code is non-zero on IPC failure <!-- tw:70b133bb-ae32-4128-90ae-169bd69dccef -->
- [ ] Happy path completes in < 200ms (no new overhead on success) <!-- tw:e961d936-7cbf-4353-ad4e-4f67086d8b83 -->
- [ ] `cargo clippy` and `cargo test` pass <!-- tw:85b35da5-feba-42a6-a170-0be2d309e00f -->
