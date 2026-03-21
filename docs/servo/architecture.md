# Servo Integration Architecture Notes

## Process Model

Brazen currently assumes a single-process Servo embedder for early development. Multi-process support is expected to land once the shell-to-engine interface is stable.

## Render Ownership

Render ownership is considered single-threaded for the initial embedder. A multi-threaded renderer will require explicit ownership, synchronization, and surface handoff rules.

## Background Tab Throttling

Background tabs will eventually use a suspend/resume strategy. The current shell already exposes `suspend` and `resume` hooks on the engine seam to make that policy explicit.

## Content-Process Isolation

The long-term model is to isolate untrusted page content from privileged shell functionality, with clear IPC boundaries and resource limits per process. The current embedder module is intentionally thin so that isolation can be introduced without rewriting the shell.
