# 30 — Premium AWH TUI Design & UX Implementation Prompt

> Standalone implementation contract for transforming the current minimal Ratatui UI into a polished Agent Workspace Control Center.

## Mission

Redesign the AWH TUI so it feels like a professional developer/agent workspace rather than a collection of basic terminal screens.

The interface must make these immediately understandable:
- current workspace, project, branch and agent/session;
- running agents, tasks, terminal jobs, MCP/connectors and Git activity;
- changed files, diffs, edits, snapshots and recent audit activity;
- failures, policy denials, conflicts, stale state and disconnected services;
- available next actions and keyboard shortcuts.

This is a presentation/interaction redesign. It must not create a second AWH application core.

## Current problem

The current TUI has a useful backend boundary but the presentation is too minimal. Improve:
- visual hierarchy;
- persistent workspace/project context;
- status visibility;
- loading/empty/error states;
- keyboard discoverability;
- navigation depth;
- terminal-space usage;
- consistency;
- agent/session/task visibility;
- command discovery;
- mutation feedback;
- responsive layouts.

Do not solve this by merely adding more text.

## Product direction

AWH TUI should feel like a command center for AI agents and their workspaces: operational clarity + developer-tool information hierarchy + runtime safety.

Principles:
1. Workspace first.
2. Activity first.
3. Keyboard first.
4. Preserve context.
5. Progressive disclosure.
6. Safe by default.
7. Fast perception.
8. Consistent interaction.
9. Never invent unavailable state.
10. Terminal-native.

## Design system

Create one reusable design system instead of styling every screen independently.

Required primitives:
- application shell;
- header;
- navigation rail/sidebar;
- content/detail panes;
- footer/action bar;
- status bar;
- panels;
- tables;
- trees;
- tabs;
- breadcrumbs;
- dialogs;
- command palette;
- notifications/toasts;
- confirmations;
- searchable lists;
- activity feed;
- diff viewer;
- log viewer;
- help overlay;
- loading/empty/error states.

Define semantic styles for normal, muted, focused, selected, active, success, warning, error, blocked, running, pending, completed, cancelled, disconnected and unknown.

Never rely on color alone. Use text/symbols too.

Support compact, normal and large terminal layouts. When space is limited, collapse secondary content instead of breaking the layout.

## Global shell

Use a consistent shell conceptually like:

    AWH | workspace/project | agent/session | branch | connection

    Dashboard
    Agents
    Tasks
    Files
    Changes
    Git
    Context
    Memory
    Skills
    MCP
    Terminal
    Audit
    Settings

    Active view / details / activity

    Navigation shortcuts | Help | Ctrl+K Commands

The exact geometry is implementation-dependent.

The shell must always expose workspace/project context, Git state, backend/connection state and global command/help access when available.

## Dashboard

Make Dashboard the operational home.

Show:
- workspace and current project;
- active agent/session;
- Git branch and dirty state;
- running/recent agents;
- task activity;
- changed files;
- recent commits;
- snapshot/edit activity where available;
- MCP, Control API, Git, terminal, connectors and remote health;
- recent canonical audit/activity events.

Only display information actually supplied by the backend.

## Navigation

Primary sections:
1. Dashboard
2. Agents
3. Tasks
4. Files
5. Changes
6. Git
7. Context
8. Memory
9. Skills
10. MCP
11. Terminal
12. Audit
13. Settings

Required:
- keyboard navigation;
- next/previous view;
- back;
- escape;
- contextual actions;
- safe selection persistence;
- stale-selection invalidation after scope changes.

Do not force repeated navigation through unrelated screens.

## Command palette

Implement a first-class command palette, preferably Ctrl+K.

It must support fuzzy search for navigation, workspace/project switching, agent/session actions, files, Git, tasks, context/memory, MCP/connectors, settings and help.

Every command needs name, category, shortcut, enabled/disabled state and disabled reason.

Commands must call the same backend/service paths as normal UI actions.

## Keyboard model

Recommended defaults:
- ? help
- Esc close modal/palette
- Ctrl+K command palette
- q quit where safe
- Tab / Shift+Tab focus traversal
- arrows or j/k list navigation
- Enter open/execute
- Space toggle
- / search/filter
- r refresh
- b back where appropriate

Do not put destructive operations behind an easy accidental key. Help must render the actual active keymap.

## Files and Changes

Files should provide:
- tree/list;
- metadata;
- search;
- open/read;
- create;
- rename;
- delete;
- Git markers;
- explicit edit/read-only state.

Changes should provide:
- changed-file list;
- staged/unstaged state;
- additions/deletions;
- selected diff;
- stage/unstage/commit;
- edit/snapshot context where available.

Diffs must support context, line numbers, hunk boundaries, horizontal scrolling and explicit large/binary indicators. Never silently hide consequential content.

## Agents and Tasks

Agent view should expose, where available:
- agent profile;
- session;
- current project;
- worktree;
- capability/policy state;
- current task;
- recent activity;
- failures;
- audit events.

Task view must clearly distinguish pending, running, completed, failed, cancelled and blocked.

Do not create another AgentRegistry, SessionStore or task database.

Task lifecycle must remain distinct from execution/session lifecycle.

## Context, Memory and Skills

Make these views visually coherent while keeping domain boundaries explicit.

Context:
- active context;
- sources;
- size;
- last update;
- edit/open.

Memory:
- recent memories;
- search;
- append;
- timestamps;
- metadata where available.

Skills:
- installed/available;
- project-enabled;
- status;
- capability implications;
- enable/disable.

Never imply these are one persistence system.

## MCP, connectors and terminal

MCP/connectors dashboard:
- name;
- transport;
- connected/disconnected;
- enabled/disabled;
- version;
- last error;
- health.

Terminal view:
- program;
- running/completed;
- exit status;
- bounded output;
- duration;
- errors;
- cancellation where supported.

Never render credentials or secret-bearing arguments.

## Audit and notifications

Audit view:
- chronological events;
- actor/agent;
- action;
- subject;
- result;
- timestamp;
- correlation ID when available;
- filtering/search;
- detail view.

Use canonical persistent audit data. Do not create a TUI audit store.

Notifications must support success/info/warning/error/blocked and must never claim success before backend confirmation.

For consequential operations show:
Requested -> Authorizing -> Executing -> Confirmed / Failed / Unknown

Unknown results after timeout require reconciliation.

## Dialogs and errors

Create reusable dialogs for confirmation, input, search, command palette, selection, error details and policy denial.

Every dialog needs title, explanation, actions, shortcuts and clear focus.

Every slow operation needs deliberate loading, success, empty, unavailable, denied, validation, conflict, timeout, disconnected and unknown-result states.

## Responsive behavior

Compact terminal:
- active view;
- primary content;
- navigation/status;
- hide secondary details.

Normal terminal:
- navigation;
- main content;
- detail/activity panel.

Large terminal:
- richer dashboard;
- simultaneous detail/activity panels;
- additional metadata.

Resize must never panic, overlap or corrupt rendering.

## Performance

Never:
- scan the whole filesystem every frame;
- run expensive Git commands every frame;
- block rendering;
- load huge files without bounds;
- render unlimited audit/history.

Use caching, bounded lists, refresh intervals, pagination/virtualization where appropriate and mutation-driven invalidation.

## Accessibility and compatibility

Support keyboard-only operation, reduced dependence on color, clear symbols, Unicode fallback, limited-color terminals and narrow widths.

## Architecture constraints

Allowed flow:

    Input -> TUI event/router -> view state/controller
      -> WorkspaceBackend -> canonical AWH services
      -> authorization/policy -> persistence/Git/MCP/terminal/audit

Forbidden:
- TUI filesystem authority;
- TUI Git authority;
- TUI task database;
- TUI AgentRegistry;
- TUI authorization;
- TUI audit store;
- TUI duplicate persistence.

Extend existing backend/view/event abstractions instead of replacing domain services.

## Existing-code forensics

Before editing inspect:
- src/tui/mod.rs
- src/tui/app.rs
- src/tui/backend.rs
- src/tui/screens/
- src/tui/remote.rs
- awh tui entrypoint
- Ratatui/Crossterm usage
- Control API
- AgentRuntime
- Task, Context, Memory, Skills
- Git/worktree
- Audit
- authorization/policy
- terminal services

Search before adding abstractions. Record what is reused, extended or replaced and why.

## Component organization

Prefer reusable components for:
- shell;
- header;
- sidebar;
- footer;
- panel;
- table;
- tree;
- dialog;
- command palette;
- notification;
- status;
- activity;
- diff;
- keymap/commands.

The exact file layout is implementation-dependent. Reuse existing files where their responsibility already matches.

## Testing

### Component
Test minimum terminal dimensions, focus, selection, loading/empty/error states and semantic status styles.

### Interaction
Test command palette, navigation, search, dialogs, confirmation, escape/back, refresh, project switching and stale selection invalidation.

### Backend integration
Verify actual canonical service calls for files, Git, projects, context, memory, skills, tasks, terminal, audit and MCP/remote where implemented.

### Security
Test unauthorized actions, stale-scope mutation, policy denial, secret leakage through notifications/audit, malformed input and destructive confirmation.

### Resize
Test very small, normal, large and rapid resize.

### Failure
Test backend error, timeout, remote disconnect, malformed response, Git failure, missing file, permission denial and unknown operation result.

## Visual regression

Because TUI quality is visual, functional tests alone are insufficient.

Where feasible, add deterministic rendering snapshots for:
1. dashboard normal;
2. dashboard compact;
3. files;
4. changes/diff;
5. agents;
6. tasks;
7. command palette;
8. confirmation dialog;
9. error dialog;
10. audit;
11. disconnected state.

Remove timestamps, random IDs, absolute paths and other nondeterministic data from snapshots.

If a true terminal snapshot harness cannot be added safely, document that limitation and provide deterministic component/render tests.

## Acceptance criteria

### Visual
- [ ] Coherent AWH design system.
- [ ] Existing minimal screens genuinely redesigned.
- [ ] Consistent hierarchy, focus, selection and status.
- [ ] Compact terminals usable.
- [ ] Large terminals use space effectively.

### UX
- [ ] Dashboard is an operational overview.
- [ ] Navigation is fast.
- [ ] Command palette exists.
- [ ] Help exposes the real keymap.
- [ ] Actions provide feedback.
- [ ] Loading/empty/error states are deliberate.
- [ ] Destructive actions require confirmation.
- [ ] Scope context remains visible.

### Architecture
- [ ] TUI remains presentation/control.
- [ ] Existing backend boundary is reused.
- [ ] No duplicate domain authority.
- [ ] Existing authorization and audit remain authoritative.
- [ ] Existing filesystem/edit/Git/task/agent/session services remain authoritative.

### Security
- [ ] Scope changes invalidate stale state.
- [ ] Unauthorized operations are rejected.
- [ ] Secrets are not unintentionally rendered.
- [ ] Destructive operations require confirmation.
- [ ] Unknown results are not misrepresented.

### Performance
- [ ] Render loop remains responsive.
- [ ] Large content/history is bounded.
- [ ] Expensive operations do not execute every frame.
- [ ] Resize is safe.
- [ ] Remote failure does not freeze the UI.

### Testing
- [ ] Component tests pass.
- [ ] Interaction tests pass.
- [ ] Backend integration tests pass.
- [ ] Security/adversarial tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Resize tests pass.
- [ ] Visual regression evidence exists where feasible.
- [ ] Repository verification gates pass.

## Verification gates

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also manually verify awh tui in small, normal and large terminals with clean/dirty Git state, projects and representative failures.

Do not claim manual verification without an interactive terminal.

## Non-goals

Do not introduce:
- new agent runtime;
- new task engine;
- new authorization;
- new filesystem service;
- new Git implementation;
- new MCP implementation;
- new audit database;
- autonomous orchestration;
- model/provider routing;
- full IDE;
- graphical desktop UI.

The objective is excellent TUI design and UX over the existing AWH architecture.

## Final report

Report:
- Feature
- Design system
- Screens redesigned
- Components added
- Keyboard model
- Responsive strategy
- Existing services reused
- Backend interfaces changed
- Security controls
- Audit behavior
- Performance controls
- Visual regression evidence
- Tests added
- Verification results
- Known limitations

## Standalone execution rule

An AI coding agent must be able to inspect the current rust branch, understand the existing TUI, implement this redesign, preserve canonical service/security boundaries, run verification gates and report evidence without depending on another prompt or PR.
