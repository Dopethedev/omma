# Security policy

## Reporting a vulnerability

Use GitHub's private reporting: **Security → Report a vulnerability** on
<https://github.com/Dopethedev/omma/security/advisories/new>. Please do not open a
public issue for a security problem.

Include what you did, what happened, and the version (`omma --version`). A
reproduction with a crafted image or a small script is the fastest path. You can
expect an acknowledgement, an assessment of impact, and a fix or a reasoned
decline.

## Supported versions

| Version | Supported |
|---|---|
| 0.2.x | yes |
| earlier | no (never released) |

## What the threat model covers

OMMA processes untrusted input — images, file paths, MCP arguments, and text that
arrives inside images — and talks to one optional network endpoint.

- **Image decoding** goes through the `image` crate under explicit limits
  (pixels, dimension, allocation, file size). A malformed or hostile image must
  produce an error, never an unbounded allocation.
- **External processes**: Tesseract receives the image on stdin. No shell is
  involved, no temporary files are written, and its output is parsed as TSV.
- **Terminal output** strips control characters from file names, OCR text, region
  content and model answers, so a crafted image cannot drive the terminal.
- **The vision endpoint** is loopback-only unless `allow_remote` is set, must use
  `https://` when remote, never follows redirects, and refuses URLs with embedded
  credentials. An image is only sent when the operator configured an endpoint and
  asked for vision.
- **Credentials** live in an environment variable (`vision.api_key_env`) or a
  literal in a 0600 file. The key is sent only as a bearer token and is redacted
  from warnings; it never reaches JSON, logs, the doctor or an error message.
- **MCP**: paths are canonicalized, restricted to `allowed_directories` when set,
  and symlink escapes are refused. Inline image bytes are decoded under the same
  limits as a file and grant no filesystem access.
- **Untrusted content is labelled**: `ocr.untrusted` and `vision.untrusted` are
  always `true`, and the vision prompt tells the model that text inside the image
  is data, not instructions.

Out of scope: the security of the vision provider you configure, the accuracy of
a model's description, and Tesseract's own parser.
