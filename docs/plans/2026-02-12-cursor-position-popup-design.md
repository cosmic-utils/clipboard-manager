# Cursor-Positioned Popups via Transparent Overlay

## Problem

When triggering clipboard popups via keyboard shortcut (D-Bus `--toggle`, `--favorites`, `--selections`), the popup appears centered on screen. Users want the popup to appear at the current mouse cursor position, enabling mouse-button-triggered workflows.

Wayland does not expose global cursor coordinates to clients. The standard `wl_pointer` only provides surface-local coordinates for the focused surface.

## Solution: Two-Phase Overlay Capture

Use a fullscreen transparent layer surface on the Overlay layer to capture the cursor position, then destroy it and create the real popup at those coordinates.

### Phase 1: Cursor Capture

1. D-Bus toggle arrives
2. Create fullscreen transparent overlay layer surface:
   - `Layer::Overlay` (above everything)
   - All-edge anchor (fills the monitor)
   - `IcedOutput::Active` (targets the monitor with the cursor)
   - `KeyboardInteractivity::None` (doesn't steal focus)
   - `input_zone: None` (accepts pointer events)
3. Overlay's view is a `mouse_area` wrapping transparent space
4. `on_move` fires with surface-local coordinates = monitor-absolute coordinates

### Phase 2: Positioned Popup

1. Store captured (x, y) coordinates
2. Destroy overlay surface
3. Create real popup as layer surface:
   - `Layer::Top`
   - `Anchor::TOP | Anchor::LEFT`
   - `margin: { top: y, left: x }`
   - `KeyboardInteractivity::Exclusive`
   - `IcedOutput::Active` (same monitor)

### Multi-Monitor

- `IcedOutput::Active` = compositor places surface on monitor with cursor
- Both overlay and popup use `Active` = same monitor
- Surface-local coordinates from overlay = correct monitor-local position
- Works regardless of monitor resolution, scaling, or arrangement

### State Machine

```
[Idle] --DbusToggle--> [CapturingCursor] --CursorCaptured(x,y)--> [PopupOpen]
                             |                                          |
                       (overlay surface)                        (positioned popup)
                             |                                          |
                       on_close_requested                      on_close_requested
                             +-----------------> [Idle] <---------------+
```

### Edge Cases

- **Escape during capture**: Overlay closes, state returns to Idle
- **Screen edges**: Clamp popup position to keep it within monitor bounds
- **Timing**: ~30ms (2-3 frames) between trigger and popup appearing
- **Overlay visibility**: Fully transparent, no visual flash

## Files Modified

- `src/app.rs` — CursorCapture state, overlay creation, cursor message handler, view_window overlay rendering
- `src/message.rs` — CursorCaptured message variant

## Alternatives Considered

1. **External tool (wl-find-cursor)**: Adds dependency, subprocess latency, multi-monitor issues
2. **Separate wayland-client thread**: Massive complexity, second Wayland connection
3. **XDG popup with anchor_rect**: Requires parent surface, can't position arbitrarily on screen
