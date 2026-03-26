# Interaction + Input Roadmap

Focuses on making the browser feel usable: accurate pointer/keyboard input, right-click context menus, zoom, and IME/clipboard.

## Pointer + Click

- [x] Normalize pointer coordinates across scale factors and viewports.
- [ ] Track hover/enter/leave events reliably for Servo.
- [x] Support left/right/middle click with correct button mapping.
- [ ] Support double-click and click count.
- [x] Add pointer capture for drag operations.
- [ ] Fix cursor updates (text, pointer, resize, grab, etc).

## Scroll + Zoom

- [x] Wire Ctrl + mouse wheel to zoom (browser zoom, not page scale only).
- [x] Add configurable zoom step and min/max bounds.
- [x] Persist per-tab zoom level.
- [x] Render UI zoom indicator and reset button.
- [x] Distinguish trackpad vs wheel scrolling.
- [x] Allow Shift + wheel to horizontal scroll.

## Keyboard + IME

- [x] Fix key mapping for common navigation keys (Backspace, Enter, Tab, Esc).
- [ ] Support Ctrl/Cmd shortcuts (Copy/Paste/Select All/Find).
- [ ] Ensure key repeat and modifiers are delivered correctly.
- [x] Integrate IME composition start/update/end with Servo.

## Context Menus + Right Click

- [x] Handle right-click events in Servo input pipeline.
- [x] Surface Servo context menu requests in UI.
- [x] Render custom context menu in egui (copy link, open in new tab, inspect).
- [x] Map context menu actions back to Servo.

## Clipboard + Drag/Drop

- [x] Bridge clipboard read/write requests (text).
- [ ] Add drag-and-drop for URLs into the address bar.
- [ ] Support image drag‑out to file (future).

## Diagnostics + Testing

- [x] Add input event logging toggle with timestamps.
- [x] Add a manual input test page (data URL) with buttons/inputs.
- [x] Add regression test for zoom behavior (Ctrl+wheel).
- [x] Add regression test for right-click context menu event.
