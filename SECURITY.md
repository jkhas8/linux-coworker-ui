# Security Policy

## Reporting a vulnerability

If you find a security issue in linux-coworker-ui, **please do not open a
public GitHub issue.** Instead:

1. Open a [private security advisory](../../security/advisories/new) on the
   repository, or
2. Email the maintainers (address listed on the GitHub profile).

Include:

- A description of the issue and the impact.
- Reproduction steps or a proof-of-concept.
- Affected versions (commit SHA is ideal).
- Suggested mitigation if you have one.

We aim to acknowledge reports within **3 business days** and to provide an
initial assessment within **7 days**. A fix and coordinated disclosure plan
will follow.

## Scope

In scope:

- The Tauri application (`src-tauri/`) and the bundled MCP server
  (`crates/mcp-linux-control/`).
- The frontend code that renders agent output (`src/`).

Out of scope:

- Vulnerabilities in upstream dependencies — please report those to the
  respective project. We'll backport fixes once they land upstream.
- Issues that require a malicious version of `claude` already on the user's
  PATH (treat the Claude Code CLI as trusted; the user installs it).

## Hardening notes for users

This app intentionally gives the agent broad authority on your desktop
(keyboard, mouse, screen, filesystem). The current default permission mode
(`bypassPermissions`) does not prompt before tool execution.

Until the approval-prompt flow ships (see roadmap §11 in
`docs/DEVELOPMENT.md`):

- Don't paste untrusted text into the composer — prompt injection can cause
  the agent to take destructive actions.
- Run inside a dedicated user account or VM for sensitive systems.
- Review the generated `.mcp.json` in `/tmp` if you're auditing what the
  agent can reach.

## Past advisories

None yet.
