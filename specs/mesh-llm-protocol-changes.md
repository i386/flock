# mesh-llm Protocol Changes for Flock

## Purpose

This document sketches the `mesh-llm` plugin protocol changes that are likely needed to support `flock` cleanly.

These changes are aimed at making `flock` straightforward to implement as:

- a periodic service advertiser
- a mesh-aware router
- a full external-backend proxy for `goosed`

`flock` is intended for private meshes only.

## Compatibility Stance

Breaking `mesh-llm` plugin v1 compatibility is acceptable.

That means:

- we do not need to preserve wire compatibility with existing plugins
- we do not need to preserve the current v1 plugin message shapes if a better protocol is cleaner
- protocol design should optimize for a better host/runtime model rather than compatibility shims

The recommended approach is to define a new protocol version and move `flock` to that version directly.

This protocol-change scope should explicitly include first-party `mesh-llm` plugins that currently rely on v1 semantics, especially the built-in blackboard plugin.

## Why v1 Is Awkward for Flock

Current v1 primitives are useful, but they are not a natural fit for `flock`’s needs.

Current strengths:

- plugin lifecycle
- RPC-style requests
- notifications
- channel messages
- bulk transfer messages
- mesh events

Current problems for `flock`:

1. no first-class service advertisement model
2. no first-class request/response stream abstraction
3. no transport-level concept that maps neatly to HTTP + SSE proxying
4. no clear registry/discovery model for plugin-exposed services
5. no explicit backpressure/cancel semantics for long-lived proxied streams

`flock` can probably be forced onto v1, but the implementation would be more fragile and more complex than necessary.

## Recommended Direction

Define a new plugin protocol version with three new first-class concepts:

1. services
2. streams
3. service-level request routing

In other words:

- plugins should be able to advertise named services through the host
- plugins should be able to receive a live service registry view
- plugins should be able to open stream-oriented request/response tunnels to a target node/plugin/service

This is the clean substrate for `flock`.

## Proposed v2 Concepts

### 1. Service Advertisement

Today `flock` would need to invent its own advertisement payloads and disseminate them through plugin messages.

Instead, v2 should let plugins register services with the host runtime directly.

Suggested model:

- plugin declares zero or more local services
- host runtime gossips those services through the mesh
- plugins receive service updates from the runtime

For `flock`, service advertisement must be visibility-aware:

- `flock` should advertise only on private meshes
- public meshes should not carry `flock` `goosed` service advertisements
- the runtime should make mesh visibility available clearly enough that `flock` can enforce this

Example service types:

- `goosed`
- future plugin-defined service names

Suggested service identity:

- `service_id`
- `plugin_id`
- `service_type`
- `node_id`
- `version`
- `metadata_json`
- `last_seen_unix_ms`

Suggested lifecycle messages:

- `AdvertiseService`
- `WithdrawService`
- `ServiceSnapshot`
- `ServiceUpserted`
- `ServiceRemoved`

This is much cleaner than encoding service discovery as ad hoc channel traffic.

### 2. Stream-Oriented Tunnels

`flock` needs to proxy:

- ordinary HTTP request/response
- long-lived SSE streams
- cancellation
- backpressure-sensitive byte transfer

v1 channel/bulk primitives are not a clean request/response tunnel abstraction.

v2 should add streams as a first-class protocol primitive.

Suggested lifecycle:

- open a stream
- send headers/metadata
- send body chunks
- half-close write side
- receive response headers
- receive response chunks
- close/reset/cancel

Suggested messages:

- `OpenStream`
- `StreamAccept`
- `StreamReject`
- `StreamData`
- `StreamClose`
- `StreamReset`
- `StreamWindowUpdate` or equivalent flow-control signal if needed

Minimum stream identifiers:

- `stream_id`
- `source_plugin_id`
- `target_plugin_id`
- `target_node_id`
- `service_id` or `service_type`

### 3. Request/Response Semantics Above Streams

For `flock`, generic streams may still be too low-level if every plugin has to reinvent HTTP tunneling.

There are two options.

#### Option A: Generic Streams Only

Pros:

- flexible
- reusable for future plugins

Cons:

- `flock` must define its own framing for HTTP request and response metadata

#### Option B: Host-Level Service Requests

Pros:

- simpler for `flock`
- direct mapping to proxied service calls

Cons:

- less generic

Suggested messages:

- `ServiceRequest`
- `ServiceResponseStart`
- `ServiceResponseChunk`
- `ServiceResponseEnd`
- `ServiceResponseError`
- `CancelServiceRequest`

For `flock`, this may actually be the better choice because the near-term target is very specifically “proxy a local `goosed` backend”.

## Recommendation: Services + Generic Streams

Recommended v2 shape:

- add first-class service advertisement
- add first-class generic streams
- let `flock` define the HTTP-over-stream framing on top

Why:

- service advertisement belongs in the host/runtime
- byte streams belong in the transport/runtime
- HTTP proxying belongs in `flock`

This keeps `mesh-llm` general while still removing the biggest obstacles.

## Proposed v2 Envelope Shape

Illustrative only:

```protobuf
message EnvelopeV2 {
  uint32 protocol_version = 1;
  string plugin_id = 2;
  uint64 request_id = 3;

  oneof payload {
    InitializeRequest initialize_request = 10;
    InitializeResponse initialize_response = 11;
    ShutdownRequest shutdown_request = 12;
    ShutdownResponse shutdown_response = 13;
    HealthRequest health_request = 14;
    HealthResponse health_response = 15;

    AdvertiseService advertise_service = 20;
    WithdrawService withdraw_service = 21;
    ServiceSnapshot service_snapshot = 22;
    ServiceUpserted service_upserted = 23;
    ServiceRemoved service_removed = 24;

    OpenStream open_stream = 30;
    StreamAccept stream_accept = 31;
    StreamReject stream_reject = 32;
    StreamData stream_data = 33;
    StreamClose stream_close = 34;
    StreamReset stream_reset = 35;

    RpcRequest rpc_request = 40;
    RpcResponse rpc_response = 41;
    RpcNotification rpc_notification = 42;

    ErrorResponse error_response = 50;
  }
}
```

The exact numbering is illustrative, not normative.

## Proposed Service Messages

Suggested shapes:

```protobuf
message ServiceDescriptor {
  string service_id = 1;
  string plugin_id = 2;
  string service_type = 3;
  string node_id = 4;
  string version = 5;
  string metadata_json = 6;
  uint64 last_seen_unix_ms = 7;
}

message AdvertiseService {
  ServiceDescriptor service = 1;
}

message WithdrawService {
  string service_id = 1;
}

message ServiceSnapshot {
  repeated ServiceDescriptor services = 1;
}

message ServiceUpserted {
  ServiceDescriptor service = 1;
}

message ServiceRemoved {
  string service_id = 1;
}
```

## Proposed Stream Messages

Suggested shapes:

```protobuf
message OpenStream {
  uint64 stream_id = 1;
  string target_node_id = 2;
  string target_plugin_id = 3;
  string target_service_id = 4;
  string metadata_json = 5;
}

message StreamAccept {
  uint64 stream_id = 1;
}

message StreamReject {
  uint64 stream_id = 1;
  string reason = 2;
}

message StreamData {
  uint64 stream_id = 1;
  bytes body = 2;
  bool end_of_stream = 3;
  string content_type = 4;
}

message StreamClose {
  uint64 stream_id = 1;
}

message StreamReset {
  uint64 stream_id = 1;
  string reason = 2;
}
```

## How Flock Would Use v2

### Advertisement

Each `flock` instance running in plugin mode with a local `goosed` would:

- register a `service_type = "goosed"` service
- publish metadata containing:
  - hostname
  - OS
  - CPU info
  - memory
  - disk
  - health
  - load
  - active chat count

The local laptop-side `flock` instance would receive the runtime’s service snapshot and updates rather than inventing its own discovery plane.

### New-Chat Placement

The local `flock` instance would:

- filter `goosed` services
- pick a target node
- create the chat by opening a stream to the selected remote `flock` instance

### Full External Backend Proxying

The local `flock` endpoint would:

- accept Goose HTTP requests locally
- frame them onto a stream to the selected remote `flock`
- remote `flock` would forward them to loopback `goosed`
- remote `flock` would stream the response back
- local `flock` would return a normal HTTP/SSE response to Goose

## What Can Stay the Same

Some v1 ideas are still good and can survive in v2:

- initialize / health / shutdown lifecycle
- RPC requests for plugin-local control operations
- host-mediated plugin startup and supervision

The core change is adding:

- service discovery
- streaming transport

## Blackboard Plugin Scope

The built-in blackboard plugin is in scope for the protocol migration.

Why:

- blackboard is already a first-party plugin in `mesh-llm`
- it depends on the current plugin runtime and transport behavior
- if plugin v1 compatibility is broken, blackboard must move with the host/runtime

Protocol planning should therefore assume:

- blackboard will be ported to plugin v2
- built-in blackboard loading remains supported
- blackboard’s mesh-sharing behavior must still work after the protocol break

## Likely mesh-llm Host Changes

The `mesh-llm` host/runtime will likely need to change in these places:

1. plugin registry and runtime state
   - store services advertised by each plugin
   - replicate service state across the mesh

2. plugin transport
   - support stream IDs and stream lifecycle
   - support routing a stream to a plugin on a specific node

3. plugin event delivery
   - deliver service snapshots and service updates
   - optionally reduce reliance on plugin-defined gossip for service discovery

4. plugin restart/recovery
   - withdraw services when a plugin dies
   - clean up open streams when a plugin dies or a node disappears

5. first-party plugin migration
   - port blackboard to the new runtime/protocol
   - verify built-in blackboard plugin loading still works

## Failure Semantics

The new protocol should define explicit failure behavior.

Recommended rules:

- if a plugin withdraws or dies, its services are removed
- if a node disappears, all services on that node are removed
- if a stream target disappears, the stream is reset with a concrete reason
- if a plugin cannot accept a stream, reject it explicitly

This matters for `flock` because pinned Goose sessions must fail clearly, not hang ambiguously.

## Migration Strategy

Because breaking compatibility is acceptable, the cleanest strategy is:

1. define plugin protocol v2
2. update `mesh-llm` host/runtime to v2
3. port the built-in blackboard plugin to v2
4. build `flock --plugin` against v2
5. port any internal example/test plugins as needed
6. ignore v1 compatibility except perhaps for temporary local development branches

This is better than trying to carry both protocols if the goal is to simplify the design.

## Minimum Protocol Work Needed for Flock

If we want the smallest set of changes that still helps a lot, the minimum useful additions are:

1. first-class service advertisement
2. first-class stream transport between plugin instances

Everything else can be built on top of that.

If we want the most ergonomic support specifically for `flock`, then:

1. service advertisement
2. stream transport
3. optional helper conventions for HTTP-over-stream framing

## Open Design Choices

These still need resolution during implementation:

1. should services be targetable by `service_id` only, or also by `service_type`?
2. should the host runtime provide service selection helpers, or should plugins always choose from snapshots themselves?
3. should flow control be explicit in the stream protocol, or can the runtime hide it initially?
4. should HTTP-over-stream be purely a `flock` concern, or should `mesh-llm` eventually expose a reusable request tunnel primitive?

## Recommendation

For `flock`, the best protocol direction is:

- break plugin v1 compatibility cleanly
- define plugin v2
- add host-managed service advertisement
- add first-class streams between plugin instances

That is the smallest protocol upgrade that makes `flock` feel natural rather than forced.
