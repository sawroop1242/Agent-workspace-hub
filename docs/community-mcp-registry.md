# Community MCP Registry

AWH supports a versioned, declarative community MCP registry.

## Manifest

The registry index is `registry/mcps/index.json` and uses schema version `1`.

Each entry should describe an MCP without embedding secrets:

- `id`
- `name`
- `description`
- `version`
- `author`
- `transport`
- `command` / `url`
- `args`
- `env`
- `homepage`
- `repository`

Environment values may reference secrets using `${secret:NAME}`. The registry must never contain actual credentials.

## Security requirements

Community entries are declarative metadata. AWH does not execute registry-provided code during installation. Users should review commands, arguments, URLs, and requested environment variables before enabling an MCP.

## Versioning

Registry entries use semantic versions. Updates should be explicit and should preserve the project's existing MCP reference ID.
