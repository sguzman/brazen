# Security Policy And Audit Roadmap

Tracks the control plane that keeps Brazen from turning into an unbounded privileged browser shell.

## Current State

- [x] Permissions are modeled as browser-mediated policy rather than raw OS access
- [x] Logging hooks exist for startup and command activity

## Policy Layer

- [x] Approval prompts with explicit action summaries
- [ ] Revocation UI and policy reset flows
- [x] Origin binding rules
- [ ] Session binding rules
- [x] Rate limiting and abuse controls
- [ ] Sandbox profiles for different browsing modes
- [ ] Trusted-client policy for local automation

## Audit Layer

- [x] Structured audit log for capability usage
- [x] Tool invocation history
- [x] Terminal command history with policy context
- [ ] Cache/data access audit trail
- [ ] Security event panel in the shell
- [ ] Exportable audit snapshots

## Isolation And Threat Model

- [ ] Separate privileged host actions from page execution
- [ ] Document hostile-page assumptions
- [ ] Define sensitive-data redaction policy for logs
- [ ] Review default-safe posture for all high-risk capabilities
