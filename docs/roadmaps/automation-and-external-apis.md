# Automation And External APIs Roadmap

Tracks local sockets, WebSocket APIs, subscriptions, and browser-state access for trusted external clients.

## Current State

- [ ] Automation endpoint config exists
- [ ] Tab and cache exposure flags exist in config
- [ ] Live automation server exists

## API Surface

- [ ] Tab enumeration API
- [ ] Tab manipulation API
- [ ] DOM query API
- [ ] Rendered-text / article-text API
- [ ] Cache metadata query API
- [ ] Cached asset body retrieval API
- [ ] Event subscription API for navigation and capability activity
- [ ] TTS and reading-queue control API
- [ ] Tab / Window screenshot API (Base64/Raw)
- [ ] Virtual Resource Mount control API
- [ ] Log-stream access API for remote/CLI introspection

## Transport And Trust

- [ ] Localhost WebSocket server
- [ ] Unix-domain / named-pipe option
- [ ] Client authentication model
- [ ] Capability checks for API callers
- [ ] Rate limiting and subscription backpressure

## Developer Experience

- [ ] Stable schema for request/response payloads
- [ ] Example CLI and scripting workflows
- [ ] API docs and local-debug tooling
- [ ] CLI Introspection Suite (`brazen introspect ...`)
      - [ ] `list-windows`: Show all running window IDs and titles
      - [ ] `list-tabs`: Show tabs grouped by window
      - [ ] `list-logs`: Stream or tail the internal application logs
      - [ ] `get-dom`: Retrieve a serialized/A11y-tree view of a specific tab
      - [ ] `interact-dom`: Send events (click, type, scroll) to DOM elements
      - [ ] `screenshot-tab`: Capture the current visual state of a tab
      - [ ] `screenshot-window`: Capture the entire window UI
