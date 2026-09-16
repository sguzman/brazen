# Blockers

Use this file for blockers that survive beyond one implementation attempt.

## Open

### ChatGPT compatibility with current engine is unverified

The repository has substantial Servo/browser work, but current September 2026 compatibility with the production ChatGPT site has not been verified.

The milestone requires a reliable ChatGPT page, not Servo completeness. If the current engine path fails, preserve the `BrowserEngine` seam and investigate the narrowest compatible engine route.

### Codex App Server Windows integration details are unverified locally

App Server is now the accepted Codex substrate, but Brazen still needs local evidence for:

- exact installed version/schema,
- child-process lifecycle and transport behavior on this Windows machine,
- shutdown/reconnect behavior,
- thread visibility/persistence behavior,
- which native events should map to endpoint status, operator approvals, provenance, or candidate routed content.

This is an integration-verification blocker, not a reason to design a replacement Codex runtime.

## Non-blocking unknowns

### Relationship to separately running Codex Desktop threads

It is not yet known whether a Brazen-owned App Server process can discover, resume, or correlate with threads simultaneously visible in the Codex Desktop app.

This is **not a Milestone 1 blocker**. Brazen may own its own App Server process. Investigate the relationship during re-entry because shared thread visibility would be useful, but do not make the product dependent on attaching to the already-running Desktop process.

## Resolved / removed from project responsibility

### Need for a custom Codex runtime

Resolved by adopting the official Codex App Server as external substrate. Brazen will not implement its own Codex agent loop, thread persistence, auth/config/model discovery, tool execution, diff/event lifecycle, or approval protocol unless a future concrete requirement cannot be met through the supported boundary.
