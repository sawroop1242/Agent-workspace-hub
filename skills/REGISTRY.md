# Agent Workspace Hub Skill Registry

A skill repository can be used as a community registry when it follows this layout:

```text
registry-repo/
├── registry.json
└── skills/
    ├── rust-development/
    │   └── SKILL.md
    └── github-pr-review/
        └── SKILL.md
```

`registry.json` contains a list of skills with their names, descriptions, versions and relative paths. The registry is a discovery/index source; the actual `SKILL.md` remains the source of truth.

## GitHub sources

A skill can be installed directly from a repository:

```text
github:owner/repository#main
```

The expected skill path is:

```text
skills/<skill-name>/SKILL.md
```

## Community sources

A future HTTP community registry can be configured with:

```text
community:https://example.com/registry
```

The registry should expose a machine-readable `registry.json` and immutable/versioned skill packages. Clients should validate skill metadata before installing it.
