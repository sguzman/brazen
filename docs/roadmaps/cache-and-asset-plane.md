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
- [ ] Request/response timing capture
- [ ] Response header storage and normalization
- [ ] Capture policy evaluation logging
- [ ] Capture denylist and allowlist by host
- [ ] Size-based body truncation with audit marker
- [ ] Capture for HTML/JSON/CSS/JS defaults
- [ ] Capture for images/fonts/media defaults

## Asset Store

- [ ] Content hashing and deduplication
- [ ] On-disk asset indexing
- [ ] Link assets to tab/session/article entities
- [ ] Pin important assets to long-term storage
- [ ] Memory-cache vs disk-cache vs archive-store controls
- [ ] Asset metadata schema (url, mime, size, hash, timestamps)
- [ ] Store content-addressed bodies under hash paths
- [ ] Separate index for headers and metadata
- [ ] Garbage-collection policy for expired assets
- [ ] Per-profile asset roots
- [ ] Asset provenance (tab/session/request ids)
- [ ] Asset integrity verification on read

## Inspection And Replay

- [ ] Cache inspector UI
- [ ] Query assets by URL, MIME, hash, or session
- [ ] Reconstruct captured sessions for replay/debugging
- [ ] Export/import captured asset sets
- [ ] Minimal CLI for asset queries
- [ ] JSON export format for assets and metadata
- [ ] Asset replay manifest format
