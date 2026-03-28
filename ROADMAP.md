# Flock Roadmap

## V1 Scope Boundary

`flock` v1 should aim for a Goose iOS style capability envelope, while using the newer session-based backend API rather than the legacy top-level `/reply` flow.

That means v1 is centered on:

- connecting Goose to one stable local `flock` endpoint
- selecting a remote backend for new chats
- creating and resuming sessions
- session-affine routing after creation
- session list/history
- streaming replies and cancellation
- the minimum provider/config/session sync required to make normal chats usable

Anything beyond that baseline belongs in roadmap work unless it is needed to preserve the core external-backend experience.

## Later Control-Plane UX

The initial `flock` implementation does not require an MCP tool layer inside `flock` plugin mode.

That is intentional. The first goal is a full Goose external-backend experience over the mesh:

- stable local `flock` endpoint
- remote `goosed` selection for new chats
- session stickiness after creation
- broad `goosed` route compatibility

## Deferred Work

These features are explicitly deferred until after the core routing/proxying design is working:

- MCP tools exposed by `flock` plugin mode
- in-chat host selection from Goose
- in-chat diagnostics for host health and bindings
- richer control-plane UX beyond CLI and config
- full desktop settings parity beyond the v1 baseline
- prompts settings parity
- full extension-management parity beyond basic session usability
- recipes UI parity
- schedules UI parity
- diagnostics bundle download and bug-report helpers
- telemetry parity
- security review
- tunnel settings parity
- gateway settings parity
- full MCP app parity if it is not required for the v1 capability target
- session import/export/delete/search parity beyond the v1 baseline
- Goose iOS support for `flock`
- distributed subagent execution across multiple `flock` nodes

## Candidate Future MCP Surface

If and when we add an MCP layer later, likely candidates are:

- `flock.current_host`
- `flock.list_hosts`
- `flock.select_host_for_next_chat`
- `flock.clear_host_selection`
- `flock.show_bindings`
- `flock.health_report`

These should be treated as follow-on UX improvements, not prerequisites for the first usable version.

## Advanced Future Option: Distributed Subagents

The initial `flock` design should keep Goose subagents on the same pinned backend as the parent session.

Later, we may want a more advanced model where `flock` helps distribute delegated work across multiple mesh nodes.

Possible future outcome:

- a parent Goose session runs on one pinned backend
- some delegated/subagent work is scheduled onto other eligible `flock` nodes
- results are returned and synthesized back into the parent workflow

Why this is deferred:

- Goose already has its own subagent model and expectations
- cross-node delegated execution introduces new correctness problems around:
  - filesystem assumptions
  - working directory consistency
  - tool availability
  - result collection and synthesis
  - failure handling across parent/child execution

This should be treated as a later orchestration feature, not part of the first `flock` implementation.

## Goose iOS

We want Goose iOS to support `flock` in the future.

Desired outcome:

- Goose iOS can connect through the same `flock` routing model
- the user can access a selected remote `goosed` over the mesh from mobile
- `flock` remains the routing layer rather than exposing `goosed` directly

This should likely reuse the same core ideas as desktop:

- a stable `flock`-managed backend surface
- remote `goosed` selection for new chats
- session stickiness after creation

Exact transport and pairing UX for iOS can be designed later.
