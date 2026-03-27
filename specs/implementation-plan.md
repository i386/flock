# Flock Implementation Plan

## Objective

Deliver a standalone `~/code/flock` workspace with:

- `flock`: the only binary, used both as the CLI and as the `mesh-llm` plugin executable via a plugin-mode flag

Keep `mesh-llm` changes limited to protocol support needed by `flock`.

Target outcome:

- Goose should see `flock` as a full external backend, not a partial proxy that only supports a small route subset.
- `flock` should only work on private meshes.
- v1 should aim for Goose iOS style capability scope, implemented on the newer session-based API.

## Workspace Layout

Planned structure:

```text
~/code/flock/
  Cargo.toml
  README.md
  specs/
    flock-spec.md
    implementation-plan.md
  flock/
    Cargo.toml
    src/main.rs
    src/...
```

## Phase 0: Scaffold

Goal:

- create the new Rust workspace
- add both crates
- prove they build

Tasks:

- create root workspace `Cargo.toml`
- create `flock/Cargo.toml`
- wire `flock` to the external `mesh-llm-plugin` crate by path
- add a minimal README

Deliverable:

- `cargo build` succeeds in `~/code/flock`

## Phase 1: Installation CLI

Goal:

- make `flock install` usable before any routing work

Tasks:

- determine `~/.mesh-llm/config.toml` path
- determine installed plugin binary target path, for example `~/.mesh-llm/flock`
- implement idempotent config editing for `[[plugin]] name = "flock"`
- implement idempotent config editing for `[flock.routing]`
- write sensible routing defaults on first install
- prompt for the node working directory during setup and write it to config
- preserve existing user-edited routing values on re-install
- support initial install and re-install safely
- print the resulting plugin path and config status

Decisions:

- start with direct file editing in `flock`
- do not depend on `mesh-llm` internals for installation logic

Deliverable:

- `flock install` places the binary in `~/.mesh-llm`
- `flock install` ensures `config.toml` has a valid plugin entry
- `flock install` ensures `config.toml` has a valid `[flock.routing]` block with defaults

## Phase 2: Plugin Skeleton

Goal:

- get `flock --plugin` loaded by `mesh-llm`

Tasks:

- create plugin mode inside the `flock` binary, modeled after the external example plugin
- respond to initialize/health
- load routing configuration from `~/.mesh-llm/config.toml`
- detect mesh visibility and refuse operation on public meshes
- maintain an in-memory state structure for:
  - local node metadata
  - discovered remote nodes
  - next-chat host preference
  - session bindings

Deliverable:

- plugin loads successfully in `mesh-llm`
- tools can be called through the plugin bridge

## Phase 3: Service Advertisement

Goal:

- advertise which nodes can front a local `goosed`

Tasks:

- define the `FlockAdvertisement` structure
- derive hostname from the local machine hostname
- collect operating system metadata
- collect CPU metadata
- collect maximum memory
- collect estimated available disk space
- collect live CPU and memory load
- publish health/load/resource metrics periodically
- process mesh events and keep a registry of live candidates
- age out stale advertisements
- respect advertisement timing and freshness thresholds from `[flock.routing]`
- estimate disk availability from the filesystem containing the configured `working_dir`
- advertise only on private meshes

Suggested state:

```text
known_hosts: Map<node_id, FlockAdvertisement>
selected_host_for_next_chat: Option<node_id>
session_bindings: Map<session_id, node_id>
```

Deliverable:

- one node can advertise itself
- another node can discover and list it
- advertisements refresh periodically and stale nodes expire automatically
- public meshes do not expose `flock` advertisements

## Phase 4: Local `goosed` Supervision

Goal:

- enable `flock` in plugin mode to manage a local `goosed`

Tasks:

- spawn `goosed agent`
- keep it bound to loopback only
- generate or load a backend secret
- health-check `/status`
- surface health in the advertisement
- supervise restarts conservatively

Deliverable:

- `flock --plugin` can manage a local `goosed`
- health state is visible to other nodes
- `goosed` is started on demand and restarted conservatively

## Phase 5: Local Stable Endpoint

Goal:

- expose one stable Goose-facing local endpoint

Tasks:

- run a loopback-only HTTP server from local `flock` in plugin mode
- use a configurable local port from `[flock.routing]`
- define local auth for Goose -> `flock`
- implement `GET /status`
- tunnel the request to a selected remote node
- forward the request to remote local `goosed`
- return the response transparently
- use structured request/response metadata plus byte streams over the new plugin transport
- reject routing if the mesh is public

Important:

- this is the first end-to-end architecture proof
- keep it narrow: `/status` only

Deliverable:

- Goose-compatible `GET /status` via local `flock` endpoint works against a remote node
- public meshes are rejected explicitly

Note:

- `/status` is only the proof point for the transport path
- the product target remains full external-backend compatibility

## Phase 6: New-Chat Placement

Goal:

- choose the best backend node before creating a new chat

Tasks:

- implement candidate filtering:
  - healthy
  - fresh
  - `goosed_available`
- implement scoring:
  - RTT
  - active chat count
  - CPU
  - memory
  - max memory
  - estimated available disk
- source thresholds and scoring weights from `[flock.routing]`
- apply explicit next-chat host selection first
- create the chat on the chosen node
- record `session_id -> node_id` once returned
- clear `next_chat_target` after it is consumed successfully

Selection rule:

- only pin after the chat is created
- only route by session after pinning

Deliverable:

- new chats are placed automatically and pinned correctly

## Phase 7: Session-Affine Proxying

Goal:

- route ordinary Goose traffic for existing sessions to the correct node

Tasks:

- implement `GET /sessions`
- implement `GET /sessions/{id}`
- route requests using `session_bindings`
- define behavior for unknown/unbound sessions
- handle restarts without corrupting bindings

Deliverable:

- basic session browsing works through `flock`

## Phase 8: `/reply` Streaming

Goal:

- support actual Goose chat traffic

Tasks:

- implement `POST /sessions/{id}/reply`
- implement `GET /sessions/{id}/events`
- implement `POST /sessions/{id}/cancel`
- preserve SSE framing
- support cancelation and disconnect cleanup
- preserve response ordering and backpressure

This is the hardest data-plane step.

Deliverable:

- Goose can chat end-to-end through `flock`

## Phase 9: Full External-Backend Route Coverage

Goal:

- support the broader `goosed` surface Goose Desktop expects in real external-backend use

Tasks:

- inventory the full set of routes Goose Desktop actually uses when pointed at an external backend
- add route forwarding for all required status/session/reply/setup/config flows
- verify that the local `flock` endpoint is interchangeable with a real external `goosed`
- eliminate assumptions that only a small fixed subset of routes matter

Deliverable:

- Goose Desktop works against `flock` as a full external backend
- the route inventory is documented and treated as required compatibility scope

## Phase 10: `flock goose`

Goal:

- make the system easy to use

Tasks:

- ensure the local stable `flock` endpoint is available
- configure Goose to use that stable local endpoint
- launch Goose Desktop if installed, else Goose CLI
- print useful status:
  - local endpoint URL
  - current next-chat target
  - installation status

Deliverable:

- `flock goose` becomes the normal entrypoint

## Phase 11: Failure Handling

Goal:

- keep the system understandable under node failure

Tasks:

- if a pinned node disappears, mark the session unavailable
- do not silently migrate an existing session
- offer the user a way to start a new chat elsewhere
- add host cooldown after repeated failures

Deliverable:

- failure behavior is explicit and predictable

## Phase 12: Protocol Review with `mesh-llm`

Goal:

- isolate the minimal protocol changes required upstream

Tasks:

- validate whether current plugin channel/bulk semantics are good enough for generic streamed HTTP/SSE proxying
- decide whether a first-class request/response tunnel primitive is needed
- decide whether service advertisement needs a formal protocol message
- include built-in blackboard migration in the upstream scope
- upstream only the minimal required changes

Deliverable:

- a bounded list of `mesh-llm` protocol changes, if any
- an explicit migration scope for the built-in blackboard plugin

Reference:

- [mesh-llm-protocol-changes.md](/Users/jdumay/code/flock/specs/mesh-llm-protocol-changes.md)

## Phase 13: First-Party Plugin Migration in `mesh-llm`

Goal:

- ensure the protocol break covers first-party plugins, not just `flock`

Tasks:

- port the built-in blackboard plugin to the new runtime/protocol
- verify default blackboard plugin resolution still works
- verify blackboard mesh-sharing behavior still works after the protocol break
- port any example/test plugins needed for development and validation

Deliverable:

- plugin v2 is usable by both `flock` and the built-in blackboard plugin

## Initial Milestones

### Milestone 1

- workspace exists
- `flock install` works
- plugin loads in `mesh-llm`

### Milestone 2

- one node advertises local `goosed`
- another node discovers it
- local `flock` endpoint proxies `/status`

### Milestone 3

- new chat placement works
- session pinning works
- `/sessions` works

### Milestone 4

- `/reply` SSE works
- Goose can run end-to-end through `flock`

### Milestone 5

- full external-backend route coverage is in place
- Goose Desktop can use `flock` as a real external backend

### Milestone 6

- `mesh-llm` plugin v2 is in place
- blackboard is ported and working under the new protocol

## Risks

### Protocol Risk

The current plugin transport may be awkward for generic HTTP/SSE tunneling. This is the biggest likely reason `mesh-llm` will need protocol changes.

### State Risk

If session bindings are lost or handled incorrectly, requests may be routed to the wrong node. This must be treated as correctness-critical.

### Goose Compatibility Risk

Goose may rely on more `goosed` routes than expected once used as a full external backend. Start with `/status`, then inventory and implement the actual route set Goose uses in practice.

## Recommended Next Work

The next implementation step should be:

1. scaffold the `~/code/flock` workspace
2. implement `flock install`
3. implement a loadable `flock --plugin` skeleton
4. define and exchange `FlockAdvertisement`
5. prove `/status` proxying end-to-end
