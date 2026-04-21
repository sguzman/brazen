# Persistence And Profiles Roadmap

Tracks config, data directories, structured local state, and multi-profile isolation.

## Current State

- [x] Platform config path resolution exists
- [x] Platform data/log/cache/profile roots are resolved
- [x] Default config generation exists
- [x] File-based persistence is the current baseline

## Persistence Layers

- [x] Structured browser-state persistence beyond config/logs (SQLite per profile)
- [x] Session persistence
- [x] History persistence
- [x] Permission-grant persistence
- [ ] Cache index persistence
- [x] Reading and knowledge persistence (reading + TTS + visit stats)

## Profiles

- [ ] Profile creation and switching
- [x] Per-profile cache roots
- [ ] Per-profile connector policies
- [ ] Per-profile automation/API settings
- [ ] Import/export profile bundles

## Migration And Durability

- [ ] Config migration strategy
- [ ] Data-versioning policy
- [ ] Backup/export workflows
