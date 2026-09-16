# Blockers

Use this file for blockers that survive beyond one implementation attempt.

## Open

### Codex desktop thread attachment is unverified

Brazen’s preferred Codex integration target is the official Codex App Server boundary. It is not yet verified whether a Brazen client can attach to, discover, or reliably correlate with threads already owned by a separately running Codex desktop app.

Do not design the milestone around direct desktop-process attachment until the re-entry evidence pass settles this.

### ChatGPT compatibility with current engine is unverified

The repository has substantial Servo/browser work, but current September 2026 compatibility with the production ChatGPT site has not been verified.

The milestone requires a reliable ChatGPT page, not Servo completeness. If the current engine path fails, preserve the `BrowserEngine` seam and investigate the narrowest compatible engine route.

## Resolved

None yet.
