# Goose External Backend Route Inventory

## Purpose

This document tracks the full set of `goosed` routes that `flock` must support to provide a real external-backend experience for Goose.

This is not meant to be speculative. It should be filled from:

- real Goose Desktop behavior against an external backend
- targeted source inspection where needed
- integration testing against `goosed`

The goal is to replace assumptions with an explicit compatibility inventory.

## Scope

`flock` should ultimately proxy the routes Goose actually relies on when pointed at an external backend.

This includes route families such as:

- status
- system info / diagnostics if used by Goose
- sessions
- reply/chat
- setup/config
- other routes required by normal external-backend usage

## Status

Current state:

- first concrete route set filled from Goose Desktop source inspection
- additional route families still need capture from live traffic and deeper settings/app flows

## Inventory Method

Recommended process:

1. run Goose Desktop against a real external `goosed`
2. capture the request traffic and route usage
3. confirm route purpose in source where unclear
4. record each required route here
5. mark which routes are:
   - required for MVP transport proof
   - required for the v1 goose-ios-style capability target
   - required for full external-backend compatibility
   - optional / nice-to-have

## V1 Interpretation

The current v1 target is intentionally narrower than full desktop parity:

- Goose iOS style capability scope
- implemented on the newer session-based API

But there is an important constraint:

- Goose Desktop must not break when attached to `flock` over the mesh

That means the route inventory needs two distinct cuts:

1. `V1 capability scope`
   - the routes that must be fully supported for the first usable `flock` release
2. `Desktop compatibility floor`
   - routes that Desktop may hit eagerly, or routes that must return a safe response so the app does not visibly break even if the full feature is deferred

The compatibility floor is not the same thing as full feature parity.

Examples:

- a route may be outside the intended v1 feature set
- but still need a safe implementation, empty response, read-only behavior, or explicit graceful error path so Goose Desktop remains usable

## V1 Capability Route Set

The current intended v1 capability set is:

- health and boot:
  - `/status`
  - `/features`
- minimal config/provider/session sync:
  - `/config/read`
  - `/agent/update_provider`
  - `/agent/update_from_session`
- session lifecycle:
  - `/agent/start`
  - `/agent/resume`
  - `/sessions`
  - `/sessions/{session_id}`
  - `/sessions/insights`
- session-based chat transport:
  - `/sessions/{id}/events`
  - `/sessions/{id}/reply`
  - `/sessions/{id}/cancel`

## Desktop Compatibility Floor

For v1, Goose Desktop should not visibly break when attached to `flock`.

At minimum, these route families need either full support or safe compatibility behavior:

- boot and renderer initialization:
  - `/status`
  - `/features`
  - `/config/read`
- ordinary chat/session flows:
  - `/agent/start`
  - `/agent/resume`
  - `/agent/update_from_session`
  - `/sessions`
  - `/sessions/{session_id}`
  - `/sessions/{id}/events`
  - `/sessions/{id}/reply`
  - `/sessions/{id}/cancel`
- low-risk session metadata:
  - `/sessions/insights`

Compatibility behavior can mean one of:

- full support
- read-only support
- empty but valid response shape
- explicit graceful failure that Desktop already handles without destabilizing the app

Routes outside this floor can remain roadmap work, but routes inside it should not fail in ways that break app startup, navigation, or normal chat use.

## Route Table

Use this table as the canonical inventory.

| Route | Method | Used By | Required Level | Compatibility Floor | Notes |
|---|---|---|---|---|---|
| `/status` | `GET` | Desktop startup / backend health | MVP | Yes | Confirmed in `ui/desktop/src/goosed.ts` via `checkServerStatus()` |
| `/features` | `GET` | Feature flags on app boot | Core | Yes | Confirmed in `ui/desktop/src/contexts/FeaturesContext.tsx` |
| `/config` | `GET` | Settings bootstrap / config context | Core | No | Full settings bootstrap is outside v1; if unsupported, Desktop must degrade safely |
| `/config/read` | `POST` | Targeted config reads in renderer/settings | Core | Yes | Used in `ui/desktop/src/renderer.tsx`; keep for minimal config/provider sync |
| `/config/upsert` | `POST` | Settings writes | Core | No | Settings parity is roadmap work |
| `/config/remove` | `POST` | Settings cleanup | Core | No | Settings parity is roadmap work |
| `/config/extensions` | `GET` | Extension list / sync bundled extensions | Core | No | Extension-management parity is deferred |
| `/config/extensions` | `POST` | Add or toggle configured extensions | Core | No | Extension-management parity is deferred |
| `/config/extensions/{name}` | `DELETE` | Remove configured extension | Core | No | Extension-management parity is deferred |
| `/config/providers` | `GET` | Provider catalog in settings | Core | No | Provider settings UI parity is deferred |
| `/config/providers/{name}/models` | `GET` | Provider model picker | Core | No | Provider settings UI parity is deferred |
| `/config/provider-catalog` | `GET` | Provider setup catalog | Optional | No | Provider setup UX only |
| `/config/provider-catalog/{id}` | `GET` | Provider template details | Optional | No | Provider setup UX only |
| `/config/check_provider` | `POST` | Provider validation | Optional | No | Settings/setup support |
| `/config/set_provider` | `POST` | Persist selected provider | Core | No | Desktop settings parity rather than v1 baseline |
| `/config/providers/{name}/oauth` | `POST` | OAuth provider setup | Optional | No | Provider-specific onboarding path |
| `/config/slash_commands` | `GET` | Mention/slash command popover | Core | No | Desktop UX parity, not required for v1 baseline |
| `/config/prompts` | `GET` | Prompts settings list | Optional | No | Settings UX only |
| `/config/prompts/{name}` | `GET` | View prompt contents | Optional | No | Settings UX only |
| `/config/prompts/{name}` | `PUT` | Save custom prompt | Optional | No | Settings UX only |
| `/config/prompts/{name}` | `DELETE` | Reset prompt | Optional | No | Settings UX only |
| `/agent/start` | `POST` | New chat / create session / start standalone app session | Core | Yes | Confirmed in `ui/desktop/src/sessions.ts` and `components/apps/StandaloneAppView.tsx` |
| `/agent/resume` | `POST` | Load an existing session into active agent state | Core | Yes | Confirmed in `ui/desktop/src/hooks/useChatStream.ts` |
| `/agent/update_working_dir` | `POST` | Change working directory from chat UI | Core | No | Nice-to-have desktop parity, not required for v1 baseline |
| `/agent/update_provider` | `POST` | Change provider/model for active agent | Core | Yes | Needed for minimum provider sync and already used by goose-ios |
| `/agent/update_session` | `POST` | Change mode for active session | Core | No | Desktop mode UX parity is deferred |
| `/agent/update_from_session` | `POST` | Sync backend agent state from loaded session | Core | Yes | Triggered automatically after session load; used by goose-ios too |
| `/agent/add_extension` | `POST` | Enable extension in active session | Core | No | Explicit extension-management parity is deferred |
| `/agent/remove_extension` | `POST` | Disable extension in active session | Core | No | Explicit extension-management parity is deferred |
| `/agent/tools` | `GET` | Tool permissions UI / MCP apps / tool count | Core | No | Tool-permission and MCP app parity are roadmap work |
| `/agent/read_resource` | `POST` | MCP resource rendering | Core | No | MCP app/resource parity is deferred |
| `/agent/call_tool` | `POST` | MCP app tool execution | Core | No | MCP app/resource parity is deferred |
| `/agent/list_apps` | `GET` | Apps view, standalone apps, platform event cache | Core | No | Apps parity is deferred |
| `/agent/export_app/{name}` | `GET` | Export MCP app | Optional | No | Apps management UX |
| `/agent/import_app` | `POST` | Import MCP app | Optional | No | Apps management UX |
| `/sessions` | `GET` | Session navigation list | Core | Yes | Confirmed in `ui/desktop/src/hooks/useNavigationSessions.ts`; also used by goose-ios |
| `/sessions/search` | `GET` | Session history filtering/search | Optional | No | Search parity is deferred |
| `/sessions/{session_id}` | `GET` | Session detail / reload conversation / edit flows | Core | Yes | Used in `useChatStream.ts`, `ChatInput.tsx`; also used by goose-ios |
| `/sessions/{session_id}` | `DELETE` | Delete session/history entry | Optional | No | Session management parity is deferred |
| `/sessions/{session_id}/export` | `GET` | Export session | Optional | No | Session management parity is deferred |
| `/sessions/import` | `POST` | Import session | Optional | No | Session management parity is deferred |
| `/sessions/insights` | `GET` | Sessions insights screen | Optional | Yes | Already used by goose-ios; low-cost inclusion in v1 |
| `/sessions/{session_id}/name` | `PUT` | Rename session | Core | No | Nice-to-have desktop parity, not required for v1 baseline |
| `/sessions/{session_id}/user_recipe_values` | `PUT` | Persist recipe parameter values | Core | No | Recipe parity is deferred |
| `/sessions/{session_id}/fork` | `POST` | Edit/fork message into new session | Core | No | Session editing/forking parity is deferred |
| `/sessions/{session_id}/extensions` | `GET` | Bottom bar extension selection | Core | No | Extension-selection UI parity is deferred |
| `/sessions/{id}/events` | `GET` | Long-lived SSE event stream for session | Core | Yes | Confirmed in `ui/desktop/src/hooks/useSessionEvents.ts`; preferred v1 streaming model |
| `/sessions/{id}/reply` | `POST` | Submit chat turn to existing session | Core | Yes | Core POST+SSE chat path in `useChatStream.ts` |
| `/sessions/{id}/cancel` | `POST` | Cancel active chat turn | Core | Yes | Needed for basic streaming UX |
| `/reply` | `POST` | Legacy/non-session reply path | Optional | No | goose-ios uses this today, but `flock` v1 should target the newer session API |
| `/system_info` | `GET` | Diagnostics / bug filing metadata | Optional | No | Diagnostics parity is deferred |
| `/diagnostics/{session_id}` | `GET` | Diagnostics bundle download | Optional | No | Diagnostics parity is deferred |
| `/mcp-ui-proxy` | `GET` | Embedded MCP UI resource proxy | Core | No | MCP UI parity is deferred unless later promoted |
| `/mcp-app-proxy` | `GET` | Sandboxed MCP app iframe proxy | Core | No | MCP app parity is deferred unless later promoted |
| `/telemetry/event` | `POST` | Desktop telemetry events | Optional | No | Telemetry parity is deferred |
| `/recipes/list` | `GET` | Recipes view / recipe selection | Optional | No | Recipes parity is deferred |
| `/recipes/create` | `POST` | Create recipe from session | Optional | No | Recipes parity is deferred |
| `/recipes/save` | `POST` | Save edited/imported recipe | Optional | No | Recipes parity is deferred |
| `/recipes/scan` | `POST` | Recipe safety scan | Optional | No | Recipes parity is deferred |
| `/recipes/parse` | `POST` | Recipe parsing | Optional | No | Recipes parity is deferred |
| `/recipes/to-yaml` | `POST` | Recipe export/render | Optional | No | Recipes parity is deferred |
| `/recipes/schedule` | `POST` | Schedule recipe | Optional | No | Recipes/schedules parity is deferred |
| `/recipes/slash-command` | `POST` | Register recipe slash command | Optional | No | Recipes parity is deferred |
| `/schedule/list` | `GET` | Schedules list | Optional | No | Schedules parity is deferred |
| `/schedule/create` | `POST` | Create schedule | Optional | No | Schedules parity is deferred |
| `/schedule/delete/{id}` | `DELETE` | Delete schedule | Optional | No | Schedules parity is deferred |
| `/schedule/{id}` | `PUT` | Update schedule cron | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/run_now` | `POST` | Run schedule immediately | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/pause` | `POST` | Pause schedule | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/unpause` | `POST` | Unpause schedule | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/kill` | `POST` | Kill running scheduled job | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/inspect` | `GET` | Inspect running scheduled job | Optional | No | Schedules parity is deferred |
| `/schedule/{id}/sessions` | `GET` | List schedule-created sessions | Optional | No | Schedules parity is deferred |
| `/dictation/config` | `GET` | Dictation settings / recorder bootstrap | Optional | No | Dictation parity is deferred |
| `/dictation/transcribe` | `POST` | Speech-to-text | Optional | No | Dictation parity is deferred |
| `/dictation/models` | `GET` | Dictation local model list | Optional | No | Dictation settings only |
| `/dictation/models/{model_id}` | `DELETE` | Delete dictation model | Optional | No | Dictation settings only |
| `/dictation/models/{model_id}/download` | `POST` | Download dictation model | Optional | No | Dictation settings only |
| `/dictation/models/{model_id}/download` | `GET` | Dictation model download progress | Optional | No | Dictation settings only |
| `/dictation/models/{model_id}/download` | `DELETE` | Cancel dictation model download | Optional | No | Dictation settings only |
| `/tunnel/status` | `GET` | Tunnel settings page | Optional | No | Tunnel parity is deferred |
| `/tunnel/start` | `POST` | Tunnel settings page | Optional | No | Tunnel parity is deferred |
| `/tunnel/stop` | `POST` | Tunnel settings page | Optional | No | Tunnel parity is deferred |
| `/gateway/status` | `GET` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/start` | `POST` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/stop` | `POST` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/restart` | `POST` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/remove` | `POST` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/pair` | `POST` | Gateway settings page | Optional | No | Gateway parity is deferred |
| `/gateway/pair/{platform}/{user_id}` | `DELETE` | Gateway settings page | Optional | No | Gateway parity is deferred |

## First Concrete Set Summary

From source inspection, the first concrete route set needed for a credible external-backend implementation is:

- bootstrap and health: `/status`, `/features`
- configuration and settings: `/config`, `/config/read`, `/config/upsert`, `/config/remove`, `/config/extensions`, `/config/providers`, `/config/providers/{name}/models`, `/config/slash_commands`
- active agent/session lifecycle: `/agent/start`, `/agent/resume`, `/agent/update_working_dir`, `/agent/update_provider`, `/agent/update_session`, `/agent/update_from_session`
- chat and session data: `/sessions`, `/sessions/{session_id}`, `/sessions/{id}/events`, `/sessions/{id}/reply`, `/sessions/{id}/cancel`, `/sessions/{session_id}/name`, `/sessions/{session_id}/user_recipe_values`, `/sessions/{session_id}/fork`, `/sessions/{session_id}/extensions`
- tool and app surfaces: `/agent/tools`, `/agent/read_resource`, `/agent/call_tool`, `/agent/list_apps`, `/mcp-ui-proxy`, `/mcp-app-proxy`

This set is large enough to cover:

- app boot against an external backend
- creating and resuming chats
- streaming replies and cancellation
- renaming/forking chats
- changing working directory, mode, provider, and extensions
- rendering and interacting with MCP apps

It is not yet the complete inventory. Recipes, schedules, dictation, prompts, diagnostics, telemetry, tunnel, and gateway routes are now cataloged here as likely follow-on families, but some still need live traffic confirmation.

For planning purposes, that concrete set is larger than the intended v1 capability set.

The correct interpretation is:

- the `V1 capability route set` defines what `flock` should aim to make fully usable first
- the `Desktop compatibility floor` defines what must not break even if full parity is deferred
- the full concrete set remains the long-term desktop parity inventory

## Source Basis

This pass is based on static inspection of Goose Desktop source and Goose server route declarations, primarily:

- `ui/desktop/src/goosed.ts`
- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/hooks/useSessionEvents.ts`
- `ui/desktop/src/hooks/useNavigationSessions.ts`
- `ui/desktop/src/components/ConfigContext.tsx`
- `ui/desktop/src/components/McpApps/McpAppRenderer.tsx`
- `ui/desktop/src/components/settings/permission/PermissionModal.tsx`
- `ui/desktop/src/components/MentionPopover.tsx`
- `ui/desktop/src/sessions.ts`
- `ui/desktop/src/schedule.ts`
- `crates/goose-server/src/routes/*.rs`

## Required Levels

Suggested meanings:

- `MVP`: needed for the first end-to-end proof
- `Core`: required for a full external-backend experience
- `Optional`: useful later but not required for initial parity

## Open Questions

Questions to resolve during the inventory:

1. Which of the optional session-management routes are actually used in the current desktop build: delete, export, import, search?
2. Is the legacy top-level `/reply` path still used anywhere outside tests or fallback flows?
3. Are any config/setup routes skipped when Desktop is attached to an already-configured external backend?
4. Which deferred desktop routes need safe compatibility shims in v1 so the app does not visibly break?
5. Are there any behavior differences for MCP app proxying when the backend is remote instead of local?

## Completion Criteria

This document is complete when:

- every route Goose Desktop uses in normal external-backend mode is listed
- each route has a required level
- each route has an implementation note or behavioral note where needed
- the list is sufficient to drive `flock` route forwarding work without guesswork
