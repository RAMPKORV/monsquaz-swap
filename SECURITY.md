# Security policy

## Supported versions

The latest 0.x release receives security fixes. Older 0.x releases may not.

## Reporting a vulnerability

Use GitHub private vulnerability reporting on
`RAMPKORV/monsquaz-swap`. Do not open a public issue for a vulnerability.

Include:

- the app version and commit;
- the operating system;
- the steps to reproduce;
- the impact;
- any suggested fix;
- whether the issue is already public.

Do not include a bot token, a private key, or participant data. Redact secrets.

If private reporting is unavailable, contact the maintainer through the
repository profile. Do not post details in a public channel.

## Scope

In scope:

- bot token exposure in the UI, logs, SQLite, git, or diagnostics;
- anonymity leaks in participant DTOs, filenames, metadata, timing, or reveal;
- path traversal, symlink escape, or archive bombs;
- parser crashes or code execution from a submitted file;
- Tauri IPC, capability, or custom-protocol issues;
- release supply-chain issues (workflow, artifact, attestation);
- data loss or duplicate delivery caused by a state bug.

Out of scope:

- a compromised host machine or a malicious OS administrator;
- Discord platform issues;
- social engineering or a participant who shares their own file;
- copyright or content disputes;
- denial of service from a user who controls the host machine.

## Response targets

| Step | Target |
|---|---|
| Acknowledge | 3 days |
| Initial triage | 7 days |
| Fix or mitigation plan | 30 days |
| Coordinated disclosure | 90 days, or sooner by agreement |

These are targets, not guarantees. A complex issue may take longer. We will
tell you the status.

## Safe harbor

We will not pursue legal action for good-faith security research that:

- uses a test account and a test guild;
- does not access another person's data;
- does not disrupt a live event;
- reports the issue privately;
- gives us reasonable time to fix it.

## Secrets

- The bot token belongs in the OS keychain. It never belongs in git, an issue,
  a screenshot, a log, or a diagnostics bundle.
- The project does not use an updater key in v1.
- If a secret is exposed, rotate it first, then report the exposure.

## More detail

The internal threat model and engineering rules are in
`ai-rules/SECURITY.md`.
