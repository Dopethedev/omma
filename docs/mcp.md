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

Input schema (`additionalProperties: false`; supply exactly one of `image_path` or
`image_base64`, expressed as `oneOf`):

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `image_path` | string | — | Local image file to analyze; stdin (`-`) is refused |
| `image_base64` | string | — | The image bytes; a `data:...;base64,` URI is accepted |
| `mime_type` | string | — | Declared media type; checked against the bytes, never trusted |
| `ocr` | boolean | true | Run OCR |
| `colors` | boolean | true | Palette + spatial grid |
| `layout` | boolean | true | Text regions from OCR boxes |
| `vision` | boolean | false | Ask the configured vision model (local or hosted) |
| `ask` | string | — | Question for the model; implies `vision` |
| `language` | string | configured | OCR languages, `eng` or `eng+deu` |
| `max_colors` | integer 1–64 | configured | Dominant colours |
| `region` | string | — | `X,Y,W,H` crop in source pixels |
| `redact` | array of strings | — | `X,Y,W,H` boxes painted out before analysis |

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

## Inline images (`image_base64`)

An agent that already holds the bytes — a pasted screenshot, a generated chart, a
file it read itself — can send them directly. No temporary file is written and no
filesystem permission is involved.

- Exactly one source per call: both or neither is a tool error.
- The payload is decoded under the same `general.max_file_size_mb` limit as a file
  (checked before and during decoding), then handed to the same decoder, the same
  `image` crate limits and the same pixel/dimension checks.
- The format comes from the bytes. `mime_type` is optional and, when it disagrees
  with the detected format, produces an `image_mime_mismatch` warning — the
  content wins.
- Malformed base64, empty payloads, SVG/XML markup, unknown formats and oversized
  images are tool errors, never crashes.
- The report's `image.filename` is `inline`.
- Inline input grants no new access: it is data the client already had, and the
  path restrictions below still govern every path.

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

`vision: true` or `ask: "..."` uses the configured endpoint, local or hosted. The
endpoint policy still applies: a remote endpoint needs `allow_remote` and
`https://`, so an MCP client cannot turn OMMA into a way of uploading images
somewhere the operator did not configure.

## Client image-forwarding, honestly

The protocol carries the bytes fine; the client decides whether to send them.

- A tool receives what the client puts in its arguments. OMMA exposes
  `image_base64` so a client *can* forward an attachment as data.
- Chat applications that paste images into a conversation and hand that
  conversation to the model do **not** automatically forward the image to an
  arbitrary MCP tool. Unless the client resolves the attachment to bytes and puts
  them in `arguments.image_base64` (or saves a file and passes `image_path`), the
  tool sees nothing.
- Verified here: any client that can send a JSON string works — the official
  TypeScript SDK, a hand-written stdio client, and the harness driving this
  project all exercised the inline path against a real model.
- Text-only agents can therefore use OMMA as their eyes: the client forwards the
  bytes, OMMA returns text, the agent reasons over it.
