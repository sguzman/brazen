# Navigation And Session Model Roadmap

Tracks tabs, windows, browsing sessions, lifecycle state, and the browser’s internal model of navigation.

## Current State

- [x] Active tab model exists
- [x] URL/title state exists
- [x] Navigation command dispatch exists
- [x] Reload command exists
- [x] Engine-originated events are surfaced into shell state

## Core Session Model

- [ ] Back/forward navigation stacks
- [ ] Pending vs committed navigation state
- [ ] Redirect chain capture
- [ ] Window and tab lineage model
- [ ] Session restore
- [ ] Crash recovery state
- [ ] Profile-bound session separation

## Browser Data Model

- [ ] Structured models for windows, tabs, frames, and documents
- [ ] Selection and focused-element state
- [ ] Download and permission-grant linkage to sessions
- [ ] Browsing-session identifiers stable across subsystems
- [ ] Revisit history and tab lineage metadata

## User-Facing Flows

- [ ] Open in new tab/window flows
- [ ] Duplicate, pin, mute, and close behaviors
- [ ] Session snapshot export/import
