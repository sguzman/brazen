# Cache And Asset Plane Roadmap

Tracks request/response capture, asset storage, replay, and the “intimate browser data” path called out in the notes.

## Current State

- [x] Cache policy config exists
- [x] Metadata/body/archive mode knobs exist in config
- [x] MIME allowlist and size threshold knobs exist in config
- [x] Basic capture and indexing implementation exists

## Capture Modes

- [x] Metadata-only capture
- [x] Selective body capture
- [x] Full archival replay capture
- [x] First-party vs third-party capture policy
- [x] Authenticated-page capture policy
- [x] Request/response timing capture
- [x] Response header storage and normalization
- [x] Capture policy evaluation logging
- [x] Capture denylist and allowlist by host
- [x] Size-based body truncation with audit marker
- [x] Capture for HTML/JSON/CSS/JS defaults
- [x] Capture for images/fonts/media defaults

## Asset Store

- [x] Content hashing and deduplication
- [x] On-disk asset indexing
- [x] Link assets to tab/session/article entities
- [x] Pin important assets to long-term storage
- [x] Memory-cache vs disk-cache vs archive-store controls
- [x] Asset metadata schema (url, mime, size, hash, timestamps)
- [x] Store content-addressed bodies under hash paths
- [x] Separate index for headers and metadata
- [x] Garbage-collection policy for expired assets
- [x] Per-profile asset roots
- [x] Asset provenance (tab/session/request ids)
- [x] Asset integrity verification on read

## Inspection And Replay

- [x] Cache inspector UI
- [x] Query assets by URL, MIME, hash, or session
- [x] Reconstruct captured sessions for replay/debugging
- [x] Export/import captured asset sets
- [x] Minimal CLI for asset queries
- [x] JSON export format for assets and metadata
- [x] Asset replay manifest format
