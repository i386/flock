# Flock Config Schema

## Purpose

This document defines the `~/.mesh-llm/config.toml` structures that `flock` reads and writes.

Scope:

- `[[plugin]]` entry for `flock` in plugin mode
- `[flock.routing]` configuration for host advertisement and new-chat placement

`flock install` should use this schema when creating defaults or backfilling missing keys.

## File Location

Default path:

```text
~/.mesh-llm/config.toml
```

If `mesh-llm` supports an override path via environment or CLI, `flock` should honor that where practical, but the default target is `~/.mesh-llm/config.toml`.

## Top-Level Structure

`flock` currently owns two config surfaces:

1. a `[[plugin]]` registration entry
2. a `[flock.routing]` table

Example:

```toml
[[plugin]]
name = "flock"
enabled = true
command = "/Users/jdumay/.mesh-llm/flock"
args = ["--plugin"]

[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = 43123
working_dir = "/Users/jdumay/code"
default_strategy = "balanced"
next_chat_target = ""
default_host_preference = ""
require_healthy_goosed = true
max_cpu_load_pct = 95
max_memory_used_pct = 95
min_disk_available_bytes = 10737418240
weight_rtt = 1.0
weight_active_chats = 15.0
weight_cpu_load = 0.7
weight_memory_used = 0.5
```

## `[[plugin]]` Entry

`flock` is a normal external `mesh-llm` plugin executable when started with a plugin-mode flag.

Required fields:

```toml
[[plugin]]
name = "flock"
enabled = true
command = "/absolute/path/to/flock"
args = ["--plugin"]
```

### Field Definitions

#### `name`

- type: `string`
- required: yes
- expected value: `"flock"`

This is the plugin identity used in `mesh-llm` plugin config.

#### `enabled`

- type: `boolean`
- required: no
- default: `true`

If omitted, `flock` should behave as enabled by default.

#### `command`

- type: `string`
- required: yes when enabled
- expected value: absolute path to installed `flock`

`flock install` should write the installed `flock` binary path here.

#### `args`

- type: `array<string>`
- required: no
- default: `[]`

Reserved for future plugin startup arguments.

For the initial design, `args` should include:

```toml
args = ["--plugin"]
```

## `[flock.routing]` Table

This table controls:

- periodic advertisement timing
- advertisement freshness windows
- local endpoint ownership details
- working-directory-based disk estimation
- new-chat host selection policy
- hard rejection thresholds
- scoring weights
- user-directed preferences

Private-mesh-only operation is not configurable here.

It is a hard invariant:

- `flock` should only advertise and route on private meshes
- if the local node is attached to a public mesh, `flock` should refuse service advertisement and remote Goose routing

### Full Schema

```toml
[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = 43123
working_dir = "/Users/jdumay/code"
default_strategy = "balanced"
next_chat_target = ""
default_host_preference = ""
require_healthy_goosed = true
max_cpu_load_pct = 95
max_memory_used_pct = 95
min_disk_available_bytes = 10737418240
weight_rtt = 1.0
weight_active_chats = 15.0
weight_cpu_load = 0.7
weight_memory_used = 0.5
```

## Field Definitions

### Advertisement Timing

#### `publish_interval_secs`

- type: `integer`
- required: no
- default: `5`
- minimum: `1`

Controls how often local `flock` republishes its node advertisement.

#### `stale_after_secs`

- type: `integer`
- required: no
- default: `20`
- minimum: must be greater than `publish_interval_secs`

Controls when a node advertisement is considered stale and should be ignored for routing.

Recommended default relationship:

- `stale_after_secs >= publish_interval_secs * 3`

### Local Endpoint / Working Directory

#### `local_port`

- type: `integer`
- required: no
- default: `43123`
- range: valid local TCP port

The loopback HTTP port exposed by `flock` running in plugin mode for Goose to use as its stable external backend.

#### `working_dir`

- type: `string`
- required: yes for a useful installation
- default written by `flock install`: user-chosen at setup time, for example `"/Users/<user>/code"`

This is the node’s working-directory root.

It is used for:

- local Goose execution context
- disk estimation

Estimated available disk for routing should be measured on the filesystem/volume that contains `working_dir`.

### Routing Strategy

#### `default_strategy`

- type: `string`
- required: no
- default: `"balanced"`

Initial allowed values:

- `"balanced"`

Reserved future values:

- `"latency-first"`
- `"capacity-first"`
- `"preferred-host"`

Initial implementation may support only `"balanced"` while validating the configured value strictly.

### User Preference / Override

#### `next_chat_target`

- type: `string`
- required: no
- default: `""`

Optional explicit target for the next new chat only.

Expected value:

- empty string for unset
- mesh `node_id`
- later, possibly hostname or display alias if resolution rules are added

This value should be consumed during new-chat placement, not used for existing sessions.

It is one-shot:

- consume it for the next successful new chat placement
- clear it afterward

#### `default_host_preference`

- type: `string`
- required: no
- default: `""`

Optional preferred target for future new chats when no one-shot target is set.

Expected value:

- empty string for unset
- mesh `node_id`
- later, possibly hostname or display alias if resolution rules are added

### Health Requirements

#### `require_healthy_goosed`

- type: `boolean`
- required: no
- default: `true`

If `true`, nodes with unhealthy local `goosed` should be rejected for new-chat placement.

### Hard Rejection Thresholds

#### `max_cpu_load_pct`

- type: `integer` or `float`
- required: no
- default: `95`
- range: `0..=100`

Reject nodes whose advertised CPU load exceeds this value.

#### `max_memory_used_pct`

- type: `integer` or `float`
- required: no
- default: `95`
- range: `0..=100`

Reject nodes whose advertised memory usage exceeds this value.

#### `min_disk_available_bytes`

- type: `integer`
- required: no
- default: `10737418240` (`10 GiB`)
- minimum: `0`

Reject or heavily penalize nodes whose estimated available disk on the filesystem containing `working_dir` is below this threshold.

Initial implementation should treat this as a hard floor.

### Scoring Weights

These weights are used for new-chat placement after filtering candidates.

Suggested score:

```text
score =
  rtt_ms * weight_rtt
  + active_chat_count * weight_active_chats
  + cpu_load_pct * weight_cpu_load
  + memory.used_pct * weight_memory_used
```

Lower is better.

#### `weight_rtt`

- type: `float`
- required: no
- default: `1.0`

#### `weight_active_chats`

- type: `float`
- required: no
- default: `15.0`

#### `weight_cpu_load`

- type: `float`
- required: no
- default: `0.7`

#### `weight_memory_used`

- type: `float`
- required: no
- default: `0.5`

### Tags

Tags are not part of the initial routing config.

Initial implementation should ignore tag-based routing entirely.

If tags become useful later, they can be added back into the routing schema in a future revision.

## Defaults Written by `flock install`

When `flock install` runs on a clean machine, it should write:

```toml
[[plugin]]
name = "flock"
enabled = true
command = "/Users/<user>/.mesh-llm/flock"
args = ["--plugin"]

[flock.routing]
publish_interval_secs = 5
stale_after_secs = 20
local_port = 43123
working_dir = "/Users/<user>/code"
default_strategy = "balanced"
next_chat_target = ""
default_host_preference = ""
require_healthy_goosed = true
max_cpu_load_pct = 95
max_memory_used_pct = 95
min_disk_available_bytes = 10737418240
weight_rtt = 1.0
weight_active_chats = 15.0
weight_cpu_load = 0.7
weight_memory_used = 0.5
```

## Merge / Backfill Rules

`flock install` must be idempotent.

Rules:

- if the `[[plugin]]` entry for `name = "flock"` is missing, add it
- if the `[[plugin]]` entry exists, preserve user-edited values unless repair is required
- if `[flock.routing]` is missing, create it with defaults
- if `[flock.routing]` exists, preserve existing values
- if newer versions add new routing keys, backfill only missing keys

`flock install` must not overwrite:

- `default_host_preference`
- `next_chat_target`
- user-adjusted weights or thresholds

## Validation Rules

`flock` should validate config values when loading.

Recommended validation:

- `publish_interval_secs >= 1`
- `stale_after_secs > publish_interval_secs`
- `local_port` must be a valid TCP port
- `working_dir` should be an absolute path
- CPU and memory percentages in `0..=100`
- weights must be finite and non-negative
- `min_disk_available_bytes >= 0`
- `default_strategy` must be recognized

If config is invalid:

- log a clear warning or error
- fall back to defaults when safe
- avoid failing in a way that bricks Goose launch unless the config is unusable

## Future Extension Points

Likely future additions:

- richer strategy options
- hostname-based preferences
- capability filters
- route-specific placement policy
- session migration policy

These should be added under `[flock.routing]` unless a separate `[flock.*]` table is clearly justified.
