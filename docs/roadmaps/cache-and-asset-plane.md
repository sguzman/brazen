# Cache And Asset Plane Roadmap

Tracks request/response capture, asset storage, replay, and the “intimate browser data” path called out in the notes.

## Current State

- [x] Cache policy config exists
- [x] Metadata/body/archive mode knobs exist in config
- [x] MIME allowlist and size threshold knobs exist in config
- [ ] No live capture or replay implementation exists yet

## Capture Modes

- [ ] Metadata-only capture
- [ ] Selective body capture
- [ ] Full archival replay capture
- [ ] First-party vs third-party capture policy
- [ ] Authenticated-page capture policy

## Asset Store

- [ ] Content hashing and deduplication
- [ ] On-disk asset indexing
- [ ] Link assets to tab/session/article entities
- [ ] Pin important assets to long-term storage
- [ ] Memory-cache vs disk-cache vs archive-store controls

## Inspection And Replay

- [ ] Cache inspector UI
- [ ] Query assets by URL, MIME, hash, or session
- [ ] Reconstruct captured sessions for replay/debugging
- [ ] Export/import captured asset sets
