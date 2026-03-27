# flock

`flock` is a standalone binary that serves two roles:

- a local CLI for installation and Goose integration
- a `mesh-llm` plugin executable when started with `--plugin`

Current status:

- workspace scaffolded
- `flock install` writes plugin registration and routing defaults
- `flock --plugin` runs a minimal private-mesh-only plugin skeleton

See `/Users/jdumay/code/flock/specs/flock-spec.md` for the design.
