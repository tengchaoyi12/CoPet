# CoPet Architecture

[简体中文](./architecture.zh.md)

CoPet is a local-first desktop companion for AI Agent CLI workflows. Its core idea is simple: Agent CLIs emit small lifecycle events, CoPet turns those events into pet behavior, and users can replace the pet and sound assets without changing the runtime.

The architecture is intentionally modular. Agent integrations, pet packages, sound packs, and Skill-generated assets all connect through stable contracts rather than hardcoded assumptions.

## Design Principles

- **Local-first by default** — runtime state, user packages, generated hooks, and preferences live on the user's machine. CoPet does not need a cloud service to observe Agent activity.
- **Agent sessions fail safe when CoPet is unavailable** — ordinary event hooks remain short-lived and fail silently. Explicit quick-action hooks may wait up to ten minutes for the user, but timeout, shutdown, and disconnect always fall back without approval.
- **Normalize at the boundary** — every Agent has different hook names and payload shapes, but the rest of the app sees a small shared event vocabulary.
- **Packages over built-ins** — built-in pets and sounds use the same package model as user-installed assets. The app scans packages instead of compiling a fixed asset list into the UI.
- **Skills are first-class creation tools** — pet and sound generation is documented as CoPet Skills, so new assets can be produced by agents and installed into the same runtime directories used by the app.
- **UI state is derived, not streamed raw** — the frontend consumes app state, derived pet state, and messages; it does not interpret raw Agent payloads.
- **Security is a design boundary** — external inputs are configuration files, local HTTP events, JSON manifests, images, and MP3s. Each one is parsed, scoped, and validated before use.

## System Shape

```text
Agent CLI hooks
  ├─ short-lived event calls
  │   └─ localhost event endpoint
  └─ bounded quick-action calls
      └─ localhost action + decision endpoints
          └─ Rust runtime core
              ├─ one-shot action registry
              ├─ Agent adapter manager
              ├─ event normalization and pet-state derivation
              ├─ config, package, and sound scanning
              └─ Tauri commands/events
                  └─ React pet window + settings window
```

The Tauri app is the only long-running process. Agent CLIs only know how to call a local helper or plugin. If the app is unavailable, the Agent session continues as if CoPet did not exist.

## Runtime Event Model

CoPet treats Agent activity as a small event stream: prompt submitted, tool started, tool finished, permission waiting, session stopped, and session errored. Adapter-specific event names are converted into this common vocabulary at the runtime boundary.

The Rust core owns state derivation. It maps events to durable UI concepts such as thinking, editing, inspecting, waiting, celebrating, or failed. The frontend then composes those Agent-derived states with local interaction state such as hover, click, long-press, drag, and idle behavior.

This separation keeps hook code small and replaceable. Hooks normally report facts; the app decides what those facts mean.

## Codex Quick Actions

Codex quick actions are a deliberately narrow bidirectional boundary. `PermissionRequest` and explicitly marked, unambiguous `Stop` events register an opaque action id, then wait on a separate decision endpoint for at most ten minutes. The UI resolves that exact id as `allowOnce`, `continueOnce`, or `fallback`; the registry consumes the decision once, so retries and concurrent tasks cannot approve another action.

Permission cards expose only display-safe command context and never offer persistent or session-wide approval. Stop messages that ask for a choice, typed input, permanent permission, destructive confirmation, or sensitive credentials are not eligible for inline continuation. Closing a card, choosing **Handle in Codex**, expiry, runtime shutdown, and lost connectivity all resolve—or behave as—`fallback`. A fallback preserves native Codex handling and never emits an approval decision.

Ordinary lifecycle events keep the original fast, silent degradation behavior. Only the two explicit quick-action hook types wait for a response, which prevents the bidirectional path from changing the latency contract of normal telemetry.

## Agent Integrations

Each supported Agent is implemented as an adapter. The adapter knows where that Agent stores hook configuration, how to detect whether CoPet is installed, and how to add or remove only CoPet-owned entries.

The shared manager handles the engineering policy around adapters: safe backups, atomic writes where possible, executable detection, repair as reinstall, and one-time auto-install on first launch. Adding a new Agent should mean adding one adapter and tests, not changing the pet renderer or settings architecture.

Current adapters cover Claude Code, Codex, Antigravity, OpenCode, Cursor, Copilot CLI, Pi, and Gemini.

## Resource Packages

Pets are Codex-compatible packages: a manifest plus a spritesheet. CoPet does not require the pet to be pixel art; any package that follows the Codex pet format and provides the expected animation rows can be selected.

Sound is modeled separately from visuals. A pet may carry its own sounds, or the user can choose a global sound pack. This lets one visual pet use different sound identities, and lets generated sound packs be reused across pets.

Built-in assets are bundled as packages. User assets live under `~/.copet`, and imports are staged before promotion so broken packages do not become active state.

## Skill Support

The `skills/` directory documents CoPet's asset-creation surface for agentic workflows:

- `copet-gen` creates a CoPet pet package by delegating generation and visual QA to `$hatch-pet`, then installing the finished package into `~/.copet/pets`.
- `copet-sound` creates a global 11-clip MP3 sound pack under `~/.copet/sounds`.

This keeps creative generation outside the application runtime. The app only needs to understand package contracts; Skills own generation quality, collision-safe ids, validation workflow, and installation. As a result, CoPet can support new generated characters and sound styles without adding app-specific generation code.

## Frontend Model

The frontend has two windows: the floating pet and the settings center. They share one app store populated from Tauri commands and kept current through Tauri events.

The pet window is deliberately lightweight: it renders the selected package, composes animation layers, plays selected sounds, and handles direct interaction. The settings window owns management workflows: choosing pets, importing packages, toggling Agent integrations, selecting sound packs, and changing preferences.

Presentational components do not own Rust IPC. Stateful app operations go through hooks or command wrappers so tests can mock the Tauri layer cleanly.

## Engineering Boundaries

Rust owns OS integration, persistence, Agent hook mutation, runtime event handling, package scanning, and native window behavior. React owns interaction ergonomics, settings workflows, animation composition, and user feedback.

Cross-boundary communication is kept narrow: typed Tauri commands for requests, Tauri events for state changes, authenticated localhost endpoints for hook events and one-shot action decisions, and package manifests for assets. This makes the codebase easier to test because each boundary has a small contract.

## Quality Strategy

The test layout mirrors the architecture:

- Rust integration tests cover runtime behavior, Agent adapters, config persistence, package import, sound scanning, i18n, and window policy.
- Playwright tests cover frontend workflows, cross-window sync, gestures, animation layering, sounds, and settings behavior.
- Package and Skill documentation describe the input/output contract for generated assets so new content can be validated without changing application code.

When changing the architecture, prefer strengthening one of these contracts over adding a special case. CoPet stays maintainable when Agent-specific details remain in adapters, generated-asset details remain in Skills, and the runtime keeps a small, stable event language.
