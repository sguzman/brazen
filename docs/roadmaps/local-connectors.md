# Local Connectors Roadmap

Tracks the brokered adapters between Brazen and local resources on the host machine.

## Current State

- [x] Connector-facing capability placeholders exist in config and permissions
- [ ] No live local connector is implemented yet

## Connector Families

- [ ] Terminal / shell broker
- [ ] File and workspace broker
- [ ] SQLite / Postgres broker
- [ ] Notes-vault broker
- [ ] Document corpus / archive broker
- [ ] OCR broker
- [ ] Git repository broker
- [ ] Media / transcript broker
- [ ] AI tool / MCP broker
- [ ] Screen / Window capture broker (AI Vision)
- [ ] Terminal-to-Chatbox bridge (e.g. bridging local CLI to web-agent inputs)

## Broker Design

- [ ] Narrow request/response contracts per connector
- [ ] Capability checks before broker execution
- [ ] Timeouts, cancellation, and streaming behavior
- [ ] Structured errors and user-facing failure surfacing
- [ ] Connector health/status reporting

## Operational Concerns

- [ ] Connector registration and lifecycle management
- [ ] Per-profile connector policies
- [ ] Test doubles for connector-heavy workflows
