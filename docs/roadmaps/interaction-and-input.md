# Interaction + Input Roadmap

Focuses on making the browser feel usable: accurate pointer/keyboard input, right-click context menus, zoom, and IME/clipboard.

## Pointer + Click

- [ ] Normalize pointer coordinates across scale factors and viewports.
- [ ] Track hover/enter/leave events reliably for Servo.
- [ ] Support left/right/middle click with correct button mapping.
- [ ] Support double-click and click count.
- [ ] Add pointer capture for drag operations.
- [ ] Fix cursor updates (text, pointer, resize, grab, etc).

## Scroll + Zoom

- [ ] Wire Ctrl + mouse wheel to zoom (browser zoom, not page scale only).
- [ ] Add configurable zoom step and min/max bounds.
- [ ] Persist per-tab zoom level.
- [ ] Render UI zoom indicator and reset button.
- [ ] Distinguish trackpad vs wheel scrolling.
- [ ] Allow Shift + wheel to horizontal scroll.

## Keyboard + IME

- [ ] Fix key mapping for common navigation keys (Backspace, Enter, Tab, Esc).
- [ ] Support Ctrl/Cmd shortcuts (Copy/Paste/Select All/Find).
- [ ] Ensure key repeat and modifiers are delivered correctly.
- [ ] Integrate IME composition start/update/end with Servo.

## Context Menus + Right Click

- [ ] Handle right-click events in Servo input pipeline.
- [ ] Surface Servo context menu requests in UI.
- [ ] Render custom context menu in egui (copy link, open in new tab, inspect).
- [ ] Map context menu actions back to Servo.

## Clipboard + Drag/Drop

- [ ] Bridge clipboard read/write requests (text).
- [ ] Add drag-and-drop for URLs into the address bar.
- [ ] Support image drag‑out to file (future).

## Diagnostics + Testing

- [ ] Add input event logging toggle with timestamps.
- [ ] Add a manual input test page (data URL) with buttons/inputs.
- [ ] Add regression test for zoom behavior (Ctrl+wheel).
- [ ] Add regression test for right-click context menu event.

## Text Input Fidelity

- [ ] Route plain text input to IME composition end events.
- [ ] Forward IME dismissals explicitly to Servo.
