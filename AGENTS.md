# Brazen Agent Operating Model

Brazen uses a **human -> director -> grunt** development model.

## Roles

### Human principal

The human owns product intent, priorities, risk tolerance, and final acceptance. The human should not be used as a manual message bus between agent surfaces.

### Director (ChatGPT)

The director owns:

- product framing and milestone boundaries
- architecture and interface decisions
- task decomposition
- acceptance criteria
- review of implementation reports, diffs, tests, and regressions
- keeping repository state legible enough that work can resume without chat history

The director may edit project/governance documentation directly. Large implementation changes should normally be delegated through an explicit task.

### Grunt / implementer (Codex)

Codex implements bounded tasks from the repository state and the current task contract. Codex may propose alternatives or blockers, but it does not silently redefine the product, expand scope, or revive dormant roadmap tracks.

**Codex may write code. Codex does not get to invent the project.**

## Repository is the source of truth

Chats are working surfaces. The repository is durable memory.

Before implementation, read in this order:

1. `docs/PRODUCT.md`
2. `docs/CURRENT_STATE.md`
3. `docs/roadmap.md`
4. the active milestone spec under `docs/milestones/`
5. `.ai/TASK.md`

After implementation, write `.ai/REPORT.md` and update `.ai/BLOCKERS.md` when necessary.

## Work protocol

1. Human chooses or approves the macro-goal.
2. Director writes one bounded task into `.ai/TASK.md`.
3. Codex implements only that task.
4. Codex records changed files, tests run, unresolved questions, and regressions in `.ai/REPORT.md`.
5. Director reviews repository evidence and either accepts the task or writes a correction task.
6. Macro-goal persists across corrections; a correction is not permission to redesign unrelated surfaces.

## Scope rules

- There is exactly **one active product milestone** at a time.
- The old per-capability roadmaps under `docs/roadmaps/` are retained as a capability inventory and historical reference. They are not simultaneous active backlogs.
- Work that does not help the active milestone is dormant unless the human explicitly promotes it.
- Prefer the smallest vertical slice that produces an observable operator benefit.
- Do not spend weeks improving general browser compatibility when a narrower engine/integration path can satisfy the active milestone behind the existing engine abstraction.
- Preserve inspectability: agent traffic, routing decisions, policy changes, and failures should become visible state rather than hidden behavior.

## Dependency / environment rules

- Rust remains the primary implementation language unless an integration boundary clearly requires another language.
- On Windows, prefer Scoop for project development dependencies.
- Do not introduce a new package/runtime manager or broad external service dependency without an explicit architectural reason.
- Local-first operation is preferred for the control plane.

## Definition of done for a task

A task is complete only when:

- acceptance criteria are demonstrably satisfied,
- relevant tests/checks have been run,
- failures or unverified assumptions are recorded,
- `.ai/REPORT.md` reflects what actually happened,
- repository docs are updated when implementation changed an architectural or product fact.
