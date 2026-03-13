# Servo Engine Integration Roadmap

Tracks the content-engine boundary, Servo embedding, render surfaces, and shell-to-engine coordination.

## Current State

- [x] `BrowserEngine` trait exists
- [x] Null backend exists for default builds
- [x] Feature-gated Servo scaffold backend exists
- [x] Engine status states include `NoEngine`, `Initializing`, `Ready`, and `Error`
- [x] Render-surface metadata is modeled

## Embedding Foundation

- [ ] Pin official Servo source revision for live integration
- [ ] Define Servo bootstrap lifecycle and process model
- [ ] Allocate real render surface / texture bridge into `egui`
- [ ] Forward viewport resize events to Servo
- [ ] Forward pointer, keyboard, scroll, and focus events
- [ ] Handle IME/text-input composition correctly

## Browser Engine Coordination

- [ ] Tab-to-engine instance mapping
- [ ] Navigation lifecycle events from Servo into shell state
- [ ] Popup, dialog, and context-menu mediation
- [ ] Crash detection and backend recovery UX
- [ ] Content-process isolation strategy
- [ ] Devtools / debugging integration plan

## Compatibility And Hardening

- [ ] Media playback behavior review
- [ ] Clipboard integration review
- [ ] Download handoff path
- [ ] Multi-profile / request-context support
- [ ] Platform-specific embedding prerequisites documented
