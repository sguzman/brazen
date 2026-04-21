# Automation And External APIs Roadmap

Tracks local sockets, WebSocket APIs, subscriptions, and browser-state access for trusted external clients.

## Current State

- [x] Automation endpoint config exists
- [x] Tab and cache exposure flags exist in config
- [x] Live automation server exists

## API Surface

- [x] Tab enumeration API
- [x] Tab manipulation API
- [x] DOM query API
- [ ] Rendered-text / article-text API
- [x] Cache metadata query API
- [x] Cached asset body retrieval API
- [x] Event subscription API for navigation and capability activity
- [ ] TTS and reading-queue control API
- [x] Tab / Window screenshot API (Base64/Raw)
- [ ] Virtual Resource Mount control API
- [ ] Log-stream access API for remote/CLI introspection

## Transport And Trust

- [x] Localhost WebSocket server
- [x] Unix-domain / named-pipe option
- [x] Client authentication model
- [x] Capability checks for API callers
- [x] Rate limiting and subscription backpressure

## Developer Experience

- [x] Stable schema for request/response payloads
- [x] Example CLI and scripting workflows
- [x] API docs and local-debug tooling
- [ ] CLI Introspection Suite (`brazen introspect ...`)
      - [ ] `list-windows`: Show all running window IDs and titles
      - [ ] `list-tabs`: Show tabs grouped by window
      - [ ] `list-logs`: Stream or tail the internal application logs
      - [ ] `get-dom`: Retrieve a serialized/A11y-tree view of a specific tab
      - [ ] `interact-dom`: Send events (click, type, scroll) to DOM elements
