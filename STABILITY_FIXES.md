# Clipboard Manager Stability Fixes

## Overview

This document tracks the stability improvements made to the COSMIC clipboard manager applet, following the plan outlined in `.github/prompts/plan-clipboardManager.prompt.md`.

## Completed Fixes (Phase 2: Panic/Unwrap Elimination)

### 1. `main.rs` - Graceful Startup Error Handling

**Problem:** Application would `panic!()` on startup failures, causing abrupt termination without useful error messages.

**Fix:** Replaced `panic!()` with `std::process::exit(1)` for:
- Config handler creation failure
- Applet run failure

**Files changed:** `src/main.rs`

---

### 2. `clipboard_watcher.rs` - Panic/Unwrap Elimination

**Problem:** Multiple `unwrap()` calls on seat lookups would panic if seat data was missing due to race conditions or protocol edge cases.

**Fixes:**
- Replaced `unwrap()` on `get_mut_seat()` calls with `if let` patterns + warning logs
- Replaced `unwrap()` on `offers.get_mut()` with proper error handling
- Changed `offers.remove()` to use `unwrap_or_else()` with fallback to empty set (treats as empty clipboard)

**Files changed:** `src/clipboard_watcher.rs`

---

### 3. `clipboard_watcher.rs` - GlobalError::InvalidId Handling

**Problem:** `GlobalError::InvalidId` would trigger a `panic!()` with the message "How's this possible?".

**Fix:**
- Added new error variant `Error::RegistryInvalidId`
- Replaced panic with recoverable error + warning log

**Files changed:** `src/clipboard_watcher.rs`

---

### 4. `clipboard.rs` - Channel Send Panic Elimination

**Problem:** `tx.blocking_send().unwrap()` and `output.send().await.unwrap()` would panic when channels were closed, causing cascading failures.

**Fixes:**
- Replaced all `unwrap()` on channel sends with `is_err()` checks
- Watcher loop now exits cleanly when receiver is dropped
- Removed `std::future::pending::<()>().await` calls that would hang forever after errors

**Files changed:** `src/clipboard.rs`

---

### 5. `clipboard_watcher.rs` - Offer Lifecycle Cleanup (Memory Leak Fix)

**Problem:** Offers were never removed from `state.offers` HashMap when replaced, causing unbounded memory growth over time.

**Fixes:**
- Modified `SeatData::set_offer()` and `set_primary_offer()` to return the old offer
- Event handlers now remove old offers from HashMap when new ones arrive
- Updated comment from "TODO: We never remove offers" to document new cleanup behavior

**Files changed:** `src/clipboard_watcher.rs`

---

### 6. `clipboard.rs` & `app.rs` - Error Classification System

**Problem:** All clipboard errors were treated the same, making it impossible to distinguish recoverable from fatal errors.

**Fixes:**
- Split `ClipboardMessage::Error` into `ErrorRecoverable` and `ErrorFatal` variants
- Added `ClipboardError::is_recoverable()` method to classify errors
- Recoverable errors (empty clipboard, communication issues, offer not found) just log and continue
- Fatal errors (missing protocol, no seats) update UI state appropriately

**Files changed:** `src/clipboard.rs`, `src/app.rs`

---

## Remaining Work (From Original Plan)

### Phase 3: Resource and Concurrency Hardening

- [ ] **Step 11:** Add cancellable clipboard worker lifecycle (shutdown token to prevent orphan `spawn_blocking` loops)
- [ ] **Step 12:** Add bounded payload policy (size limits/timeouts to prevent OOM from large clipboard data)
- [ ] **Step 13:** Move DB inserts off UI-thread `block_on` to async task pipeline
- [ ] **Step 14:** Enforce single-writer DB semantics (explicit warning when second instance detected)

### Phase 4: Recovery and Desktop Impact Containment

- [ ] **Step 16:** Add automatic reconnect with jittered backoff for recoverable Wayland failures
- [ ] **Step 17:** Ensure clipboard failure doesn't block external clients
- [ ] **Step 18:** Document fallback behavior for unsupported protocols

### Phase 5: Validation and Rollout

- [ ] Add automated tests for watcher state transitions
- [ ] Add stress/soak tests (rapid copy loops, large payloads)
- [ ] Monitor for regressions

---

## Future Enhancements (Outside Stability Scope)

### Direct Paste Feature

**Current behavior:** Click item → copies to clipboard → user manually pastes with Ctrl+V

**Proposed behavior:** Click item → copies to clipboard → automatically pastes to previously focused window

**Implementation approach:**
1. Track which window was focused before opening the clipboard popup
2. After copying, simulate Ctrl+V keypress using `zwp_virtual_keyboard_v1` protocol
3. Alternatively, use ydotool or similar input simulation

**Complexity:** Medium - requires virtual keyboard protocol integration

---

## Testing Notes

- The applet runs as two instances when cosmic-panel manages multiple outputs
- Exit code 137 (SIGKILL) during testing is usually from duplicate instances conflicting
- Use `RUST_LOG=cosmic_ext_applet_clipboard_manager=debug cargo run` for debug output
- Check logs with: `journalctl --user -f | grep clipboard`

---

## Related Files

- `src/main.rs` - Application entry point, startup error handling
- `src/app.rs` - Main application state, subscription management, clipboard event handling
- `src/clipboard.rs` - Clipboard subscription, channel management, error types
- `src/clipboard_watcher.rs` - Wayland protocol handling, offer lifecycle, seat management
- `src/db/sqlite_db.rs` - Database operations, locking
- `src/message.rs` - Application message types
