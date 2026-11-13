# CLI

```
omma [OPTIONS] [IMAGE]
omma analyze [OPTIONS] [IMAGE]
omma doctor [--config FILE]
omma mcp serve [--config FILE]
```

`omma IMAGE` and `omma analyze IMAGE` are the same command; both are supported
because both were documented at different times. Options may appear before or
after the image.

## Selecting work

By default every subsystem enabled in configuration runs. Naming a subsystem
selects it **exclusively**:

| Invocation | Runs |
|---|---|
| `omma img.png` | whatever configuration enables (OCR, colours, layout) |
| `omma img.png --no-ocr` | colours + layout |
| `omma img.png --ocr` | OCR only (plus metadata) |
| `omma img.png --colors` | colours + grid only (plus metadata) |
| `omma img.png --layout` | layout only (plus metadata) |
| `omma img.png --colors --layout` | those two |

`--ocr` together with `--no-ocr` is a usage error.

## Regions

```console
$ omma screenshot.png --region 0,0,640,400 --json
```

`X,Y,W,H` are pixel coordinates in the **source** image. The crop happens before
decoding-side analysis finishes, so:

- `image.width` / `image.height` keep the source dimensions,
- `image.analyzed_width` / `analyzed_height` describe the crop,
- `region` records `x`, `y`, `width`, `height`, `source_width`, `source_height`,
- every OCR box, region box and grid cell refers to the crop.

The region must lie inside the image; anything else exits 2.

## stdin

```console
$ cat shot.png | omma - --json
```

Read from stdin passes the same size, format and decode limits as a file.
`image.filename` is `"stdin"`.

## Output

- **Text** (default): a human report — metadata, the text, the palette, a
  terminal-sized view of the colour grid, layout regions, warnings, timings.
  The terminal grid shows every Nth cell of the same measured grid so a 64-wide
  map stays readable; JSON carries every cell.
- **JSON**: `--json` or `--format json`; `--pretty` to indent. In JSON mode
  stdout contains one JSON value and nothing else. Logs and errors go to stderr.
  `--json` together with `--format text` is refused (exit 2) rather than
  silently choosing one.
- **File**: `--output FILE` writes the rendered result there and leaves stdout
  empty.

## Vision

```console
$ omma shot.png --ask "What does the dialog say?"
$ omma shot.png --vision --model qwen2.5vl:7b
```

`--ask` sends the analyzed image to the configured endpoint; `--vision` uses
`"Describe this image."`. The description is marked `untrusted` in the report.
Vision is never called unless asked.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success, including partial results that carry warnings |
| 1 | Input, decoding or filesystem failure |
| 2 | Invalid arguments or configuration |
| 3 | A requested processing backend failed (`--ask`, or `--ocr`, produced nothing) |
| 4 | Output or serialization failure |
| 5 | MCP server startup failure |

A backend that fails as part of a *default* run is a warning and exit 0: the
report is still valid and still contains everything else.

## doctor

```console
$ omma doctor
```

Reports the version, build target, config file actually used, Tesseract version
and installed languages, OCR settings, vision endpoint/model state (never the
API key, only whether one is configured), resource limits, colour settings, MCP
restrictions and the logging level.
