# Brazen Roadmaps

Brazen needs separate roadmaps because the product is not a single linear browser backlog. The `tmp` notes describe a browser-centered capability platform with multiple orthogonal tracks: engine embedding, capability brokering, cache access, local connectors, automation APIs, knowledge workflows, and custom shell surfaces.

This file is the index. Each linked roadmap tracks one dimension and should only include work that primarily belongs to that axis.

## Active Roadmap Set

- [Shell And Workspace UX](./roadmaps/shell-and-workspace.md)
- [Servo Engine Integration](./roadmaps/servo-engine-integration.md)
- [Navigation And Session Model](./roadmaps/navigation-and-session-model.md)
- [Capability Permissions](./roadmaps/capability-permissions.md)
- [Security Policy And Audit](./roadmaps/security-policy-and-audit.md)
- [Local Connectors](./roadmaps/local-connectors.md)
- [Automation And External APIs](./roadmaps/automation-and-external-apis.md)
- [Cache And Asset Plane](./roadmaps/cache-and-asset-plane.md)
- [Extraction Pipeline](./roadmaps/extraction-pipeline.md)
- [Knowledge Plane](./roadmaps/knowledge-plane.md)
- [Media, Reading, And TTS](./roadmaps/media-reading-and-tts.md)
- [Persistence And Profiles](./roadmaps/persistence-and-profiles.md)
- [Observability And Quality](./roadmaps/observability-and-quality.md)

## Scope Notes

- Shell/UI covers browser chrome, inspectors, and operator-facing tooling surfaces.
- Engine integration covers Servo embedding, render surfaces, and content-process coordination.
- Capability permissions covers browser-mediated grants for terminal, DOM, cache, AI tools, and future local resources.
- Security/audit is separate from permissions because the `tmp` notes clearly treat policy, approval, revocation, rate limits, and audit logs as their own control plane.
- Local connectors are the brokered adapters to terminal, filesystem, databases, note stores, OCR, git, and similar host resources.
- Automation/API covers localhost or socket-facing control surfaces for tabs, cache, article text, and subscriptions.
- Cache, extraction, knowledge, and media/reading are split because each one can advance at a different pace and has distinct data shapes.
- Persistence/profiles and observability/quality stay separate because they cut across everything else and need explicit ownership.
