# Security and privacy

Lantai reads agent sessions that can contain private code and secrets, so privacy problems are
treated as security problems.

## Reporting a problem

Please report privately, not in a public issue:

- **GitHub:** [report a vulnerability](https://github.com/brant-ruan/lantai/security/advisories/new)
  (Security → Advisories → Report a vulnerability).

Examples of what to report:

- A secret, token or private path that survives redaction and reaches the database, an export, a
  report or a model request.
- Lantai writing to, moving or deleting anything in an agent's own data directory (Lantai must
  only read them).
- A way for a model response or session content to run code or read files outside what Lantai
  shows.

Describe the shape of the problem, for example "a token of the form `xyz_<40 hex>` is not
redacted". **Never include real session content, keys or tokens**; a made-up example of the same
form is enough.

## What to expect

Lantai is maintained by one person in their spare time. I aim to reply within a week and to fix
confirmed problems in the next release, crediting you in the changelog if you like.

## Supported versions

Only the latest release gets fixes while Lantai is at `0.x`.
