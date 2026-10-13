# Security policy

## Reporting a vulnerability

Please do not open a public issue for security problems. Use GitHub's private
vulnerability reporting on this repository ("Report a vulnerability" under the
Security tab). You will get an acknowledgement within a few days.

## Scope notes

- Buddy runs a local HTTP server (`127.0.0.1:7433` by default) that receives Claude
  Code hooks. Requests must carry the bearer token stored in the app's config
  file (owner-only permissions). `bind_all` is off by default; enabling it exposes the
  server to your network.
- The app edits `~/.claude/settings.json` to install hooks. It backs the file up
  once, writes atomically and refuses unexpected shapes.
- Permission prompts answered from the bubble are forwarded to Claude Code as the
  hook's decision; inputs longer than the bubble can show cannot be approved there.
