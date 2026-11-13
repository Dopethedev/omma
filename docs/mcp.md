# MCP

`omma mcp serve` runs OMMA as a Model Context Protocol server over **stdio**.
stdout carries protocol frames only; all logging goes to stderr.

## Client configuration

Generic form — any MCP client that can start a stdio server:

```json
{
  "mcpServers": {
    "omma": { "command": "omma", "args": ["mcp", "serve"] }
  }
}
```

With a specific configuration file:

```json
{
  "mcpServers": {
    "omma": {
      "command": "/home/you/.local/bin/omma",
      "args": ["mcp", "serve", "--config", "/home/you/.config/omma/config.toml"]
    }
  }
}
```

No client-specific instructions are given here because none were verified against
this build; the generic form is what the implementation is tested to speak.

## Tools

### `analyze_image`

Input schema (`additionalProperties: false`, `image_path` required):

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `image_path` | string | — | Local image file to analyze; stdin (`-`) is refused |
| `ocr` | boolean | true | Run OCR |
| `colors` | boolean | true | Palette + spatial grid |
| `layout` | boolean | true | Text regions from OCR boxes |
| `vision` | boolean | false | Ask the configured local model |
| `ask` | string | — | Question for the model; implies `vision` |
| `language` | string | configured | OCR languages, `eng` or `eng+deu` |
| `max_colors` | integer 1–64 | configured | Dominant colours |
| `region` | string | — | `X,Y,W,H` crop in source pixels |

Returns the full report described in [json-schema.md](json-schema.md), both as
text content and as `structuredContent`.

Tool annotations: read-only, non-destructive, idempotent, closed-world.

### `omma_capabilities`

No parameters. Returns tool version, schema version, supported formats, whether
OCR is available and which languages are installed, colour/grid settings, layout
state, vision configuration (endpoint and model, never the key), resource limits
and the MCP filesystem policy. Call it before `analyze_image` when an agent needs
to know what this machine can actually do.

## Errors

Two kinds, deliberately:

- **Tool errors** (`isError: true`) for anything the caller can act on: a missing
  file, a path outside the allowed roots, an unknown parameter, a bad region.
  The message is human-readable and reaches the caller.
- **Protocol errors** only for a request the server cannot route at all, such as
  an unknown tool name.

## Filesystem boundary

An MCP client must not get an unrestricted filesystem interface.

- Paths are canonicalized before use, so `.`/`..` and symlinks resolve to their
  real target.
- If `mcp.allowed_directories` is non-empty, the canonical path must sit inside
  one of them. A symlink pointing out of a root is refused, because the check
  runs after canonicalization.
- Size, dimension and allocation limits are the same as the CLI's.
- There is no shell, no command execution, no URL fetching, and no way to write
  files.

With an empty `allowed_directories` the server may read any path the OMMA process
can read. That is convenient for a single-user desktop and wrong for anything
shared — to lock it down:

```toml
[mcp]
allowed_directories = ["/home/you/Pictures", "/home/you/projects/site/assets"]
```

## Vision through MCP

`vision: true` or `ask: "..."` uses the configured endpoint. The loopback rule
still applies, so an MCP client cannot turn OMMA into a way of uploading images
to a remote host.
