# Extraction Pipeline Roadmap

Tracks turning page loads into structured extracted artifacts.

## Current State

- [x] Extraction feature flags exist in config
- [ ] No extraction pipeline is implemented yet

## Page Extraction

- [ ] Raw URL and redirect-chain capture
- [ ] DOM snapshot capture
- [ ] Readability / article extraction
- [ ] JSON-LD extraction
- [ ] Microdata extraction
- [ ] RDFa extraction
- [ ] Open Graph / metadata extraction
- [ ] Canonical URL, author, and publish-date extraction

## Media And Link Extraction

- [ ] Outbound link extraction
- [ ] Image/media candidate extraction
- [ ] Transcript candidate extraction
- [ ] Site-specific adapters for difficult pages

## Pipeline Behavior

- [ ] Trigger extraction on navigation commit
- [ ] Queue background extraction work
- [ ] Retry and failure handling
- [ ] Store extraction provenance and timestamps
