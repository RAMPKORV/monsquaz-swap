# Security and anonymity rules

The event promises **participant-to-participant anonymity**, not host-proof or
platform-proof anonymity. The host machine, server admins, Discord staff, and
anyone with the bot token can see more. State this in the rules.

## Trust boundaries

```
participant device ── Discord client ── TLS ── Discord ── gateway/REST
                                                        │
host OS (keychain, logs, notifications, backups) ── Tauri main ── UI
                                                        │
                                              SQLite + attachment store
```

Forgotten channels: logs, OS notifications, crash reports, backups, screen
sharing, clipboards, thumbnails, file mtimes, and guild admins.

## Bot token

- Store only in the OS keychain (`keyring` crate). Never in SQLite, JSON,
  `.env`, localStorage, logs, URLs, crash reports, or the WebView.
- Linux without Secret Service: refuse to persist, or use an explicit
  less-secure fallback with a warning. Never silently store plaintext.
- Validate with `GET /users/@me`; show the bot name/avatar, not the token.
- Redact `Authorization: Bot ...` and token-like strings from logs.
- Rotate on suspicion; document the fix-it path.

## Anonymity

- Separate owner/author tables from participant-facing projections. A recipient
  query never joins them.
- Every participant DTO passes a redaction assertion: no owner ID, display
  name, original filename, title, comments, or full schedule before reveal.
- Opaque track codes, not owner-derived or submission-order IDs.
- Outbound filenames are `S7F3A.it` — no version/holder/original name. Host
  storage may include the holder for recognition.
- Download attachments immediately; never forward CDN URLs. The URL/filename
  are identity-bearing and expire.
- Mask or remove owner/author fields in logs at default level.
- Batch delivery at fixed release windows; do not notify on upload. Randomize
  notification jitter independently of upload time.
- Public progress is status + countdown; no per-person counts before deadline.
  Aggregates use a k-threshold (recommend k≥5) and are host-only when small.
- Do not publish file hashes pre-reveal (confirmation/dedup attacks). Commit to
  the schedule/policy, not the content.
- Reveal order is HMAC-sorted, not submission order; publish atomically.
- Group membership is hidden; the visible-schedule option exposes assignments
  only, never owners/titles.
- Comments/questionnaire are host+reveal only by default; publish all at reveal
  with host redaction; warn that writing style can leak identity.
- Fill-in offers use the same template/timing as regular offers; do not label a
  slot as "rescue" in public.
- Screen-share/stream-safe mode hides the mapping, redacts IDs, and shows only
  the public presenter view. Mapping access is audited.
- If the host participates, disclose that the host sees the mapping.
- Small events cannot be anonymous against mutual inference; warn below 5
  participants and require an explicit override (the host may still run).

## Files

- Default allowlist: modules, audio, zip; per-event override. Block executables
  and scripts regardless.
- Enforce the 20 MiB cap for bot redistribution. Reject with alternatives;
  external links are downloaded by the bot when possible, else host-mediated.
- Magic-byte and extension validation; quarantine mismatches.
- Path safety: never use the client filename; canonicalize; reject `..`,
  absolute paths, symlinks, NULs, and reserved Windows names. NFC-normalize and
  strip bidi controls.
- Archives: no auto-extract; cap entries, depth, and expansion ratio; reject
  zip bombs.
- Parse modules in a crash-isolated child process (openmpt123 or a sandboxed
  parser) with time/memory limits; no network.
- Never auto-open a submitted file. Use `open folder`/preview only.
- Metadata scrub: strip names, messages, AUTH, edit history, tracker stamps,
  external sample paths, and audio/EXIF tags where safe. A/B render; fail
  closed if playback changes.
- Keep untouched originals in a host-only vault; distribute scrubbed versions.
- Normalize mtimes and archive timestamps; do not leak upload time.

## Tauri / IPC

- `contextIsolation`, `sandbox`, no Node in the WebView; capabilities limited to
  `core:default` plus explicitly needed plugins. No `fs`, `shell`, `http`, or
  `sql` plugins.
- File bytes and absolute paths never cross IPC. Use opaque file IDs, scoped
  custom protocols, and main-process path handles.
- Validate every command argument; return typed errors with no paths/secrets.
- Deny navigation and new windows; allow external URLs only from an allowlist.
- Harden CSP before public release (currently `null` in the scaffold).
- Single-instance lock; no second bot session on the same token.

## Discord and ToS

- BYO bot token, used only with the host's own application. Never a self-bot or
  user token. Never ship a shared token in a binary.
- No privileged intents required: DMs with the app and slash-command attachments
  are enough. Do not ask for `MESSAGE_CONTENT`.
- Capture consent at join; DMs are event-scoped. Do not bulk-DM users who did
  not join.
- Guild admins can read channels/private threads; do not promise otherwise.
- Do not treat Discord as durable storage; keep local copies and backups.

## Privacy

- Collect only Discord user ID, display name, event role, assignment/credits,
  files, and the minimum technical logs.
- Retention: keep event data until the host deletes/archives it; transient DM
  payloads are dropped after ingest. Backups expire; document the period.
- Self-service export and delete; hash-only audit after deletion where needed.
- No telemetry or crash reporting by default. A host-triggered diagnostics
  bundle is redacted and opt-in.
- Publish a privacy note in the app and event rules; the host is the data
  controller for their event.

## Incident checklist

1. Stop the bot / pause the event.
2. Preserve logs and the audit ledger; do not delete evidence.
3. Rotate the bot token if exposed.
4. Identify affected participants and the leak vector.
5. Notify privately; do not amplify in public.
6. Fix, add a regression test, record the decision.
7. If a file is malicious, quarantine it and warn recipients.

## Security review gates

A change is `full` ceremony if it touches: the seen ledger, scheduling
invariants, anonymity projections, token/keychain, file parsing/scrubbing, IPC,
migrations, backups/restores, or reveal. It needs an adversarial review and a
rollback/restore check.
