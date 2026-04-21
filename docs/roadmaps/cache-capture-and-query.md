# Cache Capture + Query Roadmap

Focuses on transparent, per-asset caching from real browsing, with clear visibility and query tooling.

## Capture Pipeline (Servo → AssetStore)

- [ ] Tap Servo network events to receive request/response metadata.
- [ ] Map Servo request IDs to tab/session IDs.
- [ ] Capture response headers and status codes for each asset.
- [ ] Normalize MIME types (content-type parsing + fallbacks).
- [ ] Record every request as a distinct asset entry.
- [ ] Support empty-body assets (e.g., 204/304) with metadata only.
- [ ] Track request start/finish timestamps for duration metrics.
- [ ] Capture body bytes for HTML, CSS, JS, JSON, images, SVG, fonts, audio, video.
- [ ] Respect cache policy controls for third-party and authenticated assets.
- [ ] Enforce per-asset size caps with truncated flagging.
- [ ] Record redirection chain metadata as separate assets.
- [ ] Deduplicate bodies by hash while preserving per-asset entries.
- [ ] Persist request/response headers alongside metadata records.
- [ ] Add explicit storage mode (memory/disk/archive) per asset record.
- [ ] Emit structured tracing for capture decisions and outcomes.

## Query + Visibility

- [ ] Expose cache stats (entries, total bytes, unique blobs, capture ratio).
- [ ] Add query filters for URL, MIME, session, tab, and status.
- [ ] Add “asset detail” view (headers, timings, hash, storage path).
- [ ] Add “recent assets” timeline view.
- [ ] Add search by content hash for dedupe visibility.
- [ ] Surface cache state in the status panel (last capture, last error).

## CLI + Export

- [ ] Extend `brazen cache` to print capture summary per asset.
- [ ] Add `brazen cache --list` with filters (URL/MIME/session).
- [ ] Add `brazen cache --show <asset_id|hash>` for full metadata.
- [ ] Add export to JSONL and a compact summary report.
- [ ] Add import/merge with collision handling.

## Policy + Config

- [ ] Add config for capture-all vs selective modes.
- [ ] Add explicit MIME allow/deny lists with glob support.
- [ ] Add per-host capture policy overrides.
- [ ] Add config for “store bodies always” vs “metadata-only”.
- [ ] Add config for max total cache size with GC strategy.
- [ ] Add config for “no-dedupe” mode for strict per-asset storage.

## Testing

- [ ] Unit tests for MIME parsing and policy decisions.
- [ ] Unit tests for body dedupe with distinct asset entries.
- [ ] Integration test: local server with HTML + CSS + JS + images; verify per-asset records.
- [ ] Integration test: redirect chain produces multiple assets.
- [ ] CLI tests for list/show/export commands.
