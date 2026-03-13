# Automation And External APIs Roadmap

Tracks local sockets, WebSocket APIs, subscriptions, and browser-state access for trusted external clients.

## Current State

- [x] Automation endpoint config exists
- [x] Tab and cache exposure flags exist in config
- [ ] No live automation server exists yet

## API Surface

- [ ] Tab enumeration API
- [ ] Tab manipulation API
- [ ] DOM query API
- [ ] Rendered-text / article-text API
- [ ] Cache metadata query API
- [ ] Cached asset body retrieval API
- [ ] Event subscription API for navigation and capability activity
- [ ] TTS and reading-queue control API

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
