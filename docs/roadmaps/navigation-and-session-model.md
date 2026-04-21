# Navigation And Session Model Roadmap

Tracks tabs, windows, browsing sessions, lifecycle state, and the browser’s internal model of navigation.

## Current State

- [ ] Active tab model exists
- [ ] URL/title state exists
- [ ] Navigation command dispatch exists
- [ ] Reload command exists
- [ ] Engine-originated events are surfaced into shell state
- [ ] Back/forward commands are modeled in the shell

## Core Session Model

- [ ] Back/forward navigation stacks
- [ ] Pending vs committed navigation state
- [ ] Redirect chain capture
- [ ] Window and tab lineage model
- [ ] Session restore
- [ ] Crash recovery state
- [ ] Profile-bound session separation
- [ ] Session file format versioning
- [ ] Session JSON persistence

## Browser Data Model

- [ ] Structured models for windows, tabs, frames, and documents
- [ ] Selection and focused-element state
- [ ] Download and permission-grant linkage to sessions
- [ ] Browsing-session identifiers stable across subsystems
- [ ] Revisit history and tab lineage metadata
- [ ] Navigation history stored per tab

## User-Facing Flows

- [ ] Open in new tab/window flows
- [ ] Duplicate, pin, mute, and close behaviors
- [ ] Session snapshot export/import
- [ ] Active tab switching UI
- [ ] Profile-based session path usage
- [ ] Crash recovery flag persisted in snapshots
