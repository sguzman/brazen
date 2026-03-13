# Capability Permissions Roadmap

Tracks browser-mediated grants for sensitive capabilities, following the same conceptual model as browser permissions.

## Current State

- [x] Capability-oriented permission policy exists
- [x] Default decision model supports `allow`, `ask`, and `deny`
- [x] Capability defaults are configurable in TOML
- [x] Permission state is visible in the shell

## Capability Surface

- [x] Terminal execution capability placeholder exists in config
- [x] DOM read capability placeholder exists in config
- [x] Cache read capability placeholder exists in config
- [x] Tab inspection capability placeholder exists in config
- [x] AI tool usage capability placeholder exists in config
- [ ] Terminal output-read capability
- [ ] File / workspace access capabilities
- [ ] Database / notes-vault capabilities
- [ ] OCR and media-transcript capabilities

## Grant Model

- [ ] Origin-scoped grants
- [ ] Session-scoped grants
- [ ] Profile-scoped grants
- [ ] One-shot approval prompts
- [ ] Revocation and deny-remembering rules
- [ ] Dry-run and preview execution modes
- [ ] Capability-specific argument constraints

## Product Surfaces

- [ ] Runtime prompt UI
- [ ] Grant history UI
- [ ] Capability policy editor UI
- [ ] Programmatic checks for automation / API clients
