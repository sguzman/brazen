# Automation And External APIs Roadmap

Tracks local sockets, WebSocket APIs, subscriptions, and browser-state access for trusted external clients.

## Current State

- [x] Automation endpoint config exists
- [x] Tab and cache exposure flags exist in config
- [x] Live automation server exists

## API Surface

- [x] Tab enumeration API
- [x] Tab manipulation API
- [ ] DOM query API
- [ ] Rendered-text / article-text API
- [x] Cache metadata query API
- [ ] Cached asset body retrieval API
- [x] Event subscription API for navigation and capability activity
- [ ] TTS and reading-queue control API
- [ ] Tab / Window screenshot API (Base64/Raw)
- [x] Virtual Resource Mount control API
- [x] Log-stream access API for remote/CLI introspection

## Transport And Trust

- [x] Localhost WebSocket server
- [x] Unix-domain / named-pipe option
- [x] Client authentication model
- [x] Capability checks for API callers
- [x] Rate limiting and subscription backpressure

## Developer Experience

- [ ] Stable schema for request/response payloads
- [ ] Example CLI and scripting workflows
- [ ] API docs and local-debug tooling
- [x] CLI Introspection Suite (`brazen introspect ...`)
      - [x] `list-windows`: Show all running window IDs and titles
      - [x] `list-tabs`: Show tabs grouped by window
      - [x] `list-logs`: Stream or tail the internal application logs
      - [x] `get-dom`: Retrieve a serialized/A11y-tree view of a specific tab
      - [ ] `interact-dom`: Send events (click, type, scroll) to DOM elements
      - [x] `screenshot-tab`: Capture the current visual state of a tab
      - [ ] `screenshot-window`: Capture the entire window UI
