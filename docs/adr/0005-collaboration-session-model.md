# ADR-0005: Collaboration / session model

- **Status:** Accepted
- **Date:** 2026-10-01

## Context
Several users must be able to work on one project at the same time, like players on a game server (DESIGN_GOALS §4). Single-user use must take the same code path.

## Options considered
1. File locking / check-out — simple, but no real concurrent work.
2. CRDTs (automatic merge) — good for text, but can produce merges that are valid data yet invalid engineering (e.g. a span referencing a deleted structure, broken constraints).
3. **Authoritative server with a validated, ordered command log.**

## Decision
- **Authoritative server** (`ols-session`) owns the model; clients send commands, the server validates, applies atomically, appends to the log and broadcasts change sets. Single-user = in-process server.
- **Optimistic client prediction**: edits apply instantly locally, are confirmed by the server, rolled back on rejection.
- **Conflicts**: per-object versions; stale destructive edits are rejected with a clear message; soft locks on a section while being dragged; presence indicators.
- **Undo (confirmed)**: per user — each person undoes only their own changes, via inverse commands.
- **Admin rollback (confirmed)**: admins can roll back other users' commands, revisions, or everything after a point in time.
- **Roles & permissions (confirmed, detailed later)**: default roles viewer / designer / checker-approver / admin, custom roles from fine-grained permissions, scoped per project, area/chunk and object type; some actions admin-only; all admin actions logged. See DESIGN_GOALS §4.
- **Compute**: server computes authoritative results; clients compute local previews with the same engine while dragging.
- **Interest management**: clients receive only the chunks they view.
- **Transport**: QUIC (`quinn`) for desktop; WebSocket fallback (firewalls, future web client); versioned compact binary messages.
- **Security**: TLS + per-session access code in v1; OIDC/SSO later.
- **History**: the log is the basis for git-like version control — revisions, branches for design alternatives, merge, tags (DESIGN_GOALS §7).

## Consequences
- Every edit in the codebase must be a command: no direct mutation of the model from the GUI.
- Requires a running session even offline (in-process, invisible to the user).
