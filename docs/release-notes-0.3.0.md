# Omma 0.3.0

This release adds image redaction to the CLI and MCP server.

- `--redact X,Y,W,H` covers a rectangle before text, layout, and color analysis or an optional vision request. Repeat the flag to cover multiple areas. With a crop, coordinates are relative to that crop.
- `--redact-auto` uses local Tesseract OCR to find and cover card numbers, email addresses, and phone numbers. Detection depends on what OCR can read. The command stops before sending an image to vision if OCR fails or a requested language pack is missing.
- `redactions` in the JSON report records the covered coordinates and `replacement: "blank"`. Automatically detected areas also include a `source` such as `email`.
- MCP clients can use `redact` and `redact_auto` with `analyze_image` for both file and inline images.

```sh
omma screenshot.png --json --redact 24,36,280,42
omma screenshot.png --json --redact-auto
```

The release includes Linux x86-64, macOS arm64 and x86-64, and Windows x86-64 binaries with SHA-256 checksums. The Linux binary requires glibc 2.39 or later. Automatic redaction requires Tesseract and the requested language packs.
