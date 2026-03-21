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
- [ ] Document Servo build prerequisites per platform (Linux packages, gfx backend, toolchains)
- [ ] Add `servo` feature dependency wiring to pull Servo sources via git
- [ ] Add a compile-time guard that errors when Servo sources are missing
- [ ] Implement a thin Servo embedder module with explicit init/shutdown
- [ ] Allocate real render surface / texture bridge into `egui`
- [ ] Implement a swap-chain or surface abstraction for Servo output
- [ ] Decide on single-thread vs multi-thread render ownership and document it
- [ ] Forward viewport resize events to Servo
- [ ] Forward pointer, keyboard, scroll, and focus events
- [ ] Handle IME/text-input composition correctly
- [ ] Implement clipboard integration between Servo and shell
- [ ] Handle high-DPI scale-factor changes mid-session
- [ ] Map window occlusion/minimize/restore events to Servo lifecycle hooks

## Browser Engine Coordination

- [ ] Tab-to-engine instance mapping
- [ ] Per-tab engine lifecycle (create, suspend, resume, destroy)
- [ ] Background tab throttling strategy
- [ ] Navigation lifecycle events from Servo into shell state
- [ ] Expose load progress and document-ready milestones
- [ ] Capture title, favicon, and metadata updates from Servo
- [ ] Link navigation events to command log and history model
- [ ] Popup, dialog, and context-menu mediation
- [ ] New-window and target-blank routing policy
- [ ] Crash detection and backend recovery UX
- [ ] Clean shutdown and crash-dump collection path
- [ ] Content-process isolation strategy
- [ ] Define resource limits per tab/process
- [ ] Devtools / debugging integration plan
- [ ] Devtools transport selection and security constraints

## Compatibility And Hardening

- [ ] Media playback behavior review
- [ ] Audio output device selection and policy
- [ ] Clipboard integration review
- [ ] Download handoff path
- [ ] Cookie and storage persistence handshake with app profiles
- [ ] Multi-profile / request-context support
- [ ] Service worker and cache behavior alignment review
- [ ] Mixed-content and security warning surfacing
- [ ] TLS error handling and user messaging
- [ ] Platform-specific embedding prerequisites documented
