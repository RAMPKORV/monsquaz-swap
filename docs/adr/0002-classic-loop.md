# ADR 0002 — Classic event production loop and live runtime

**Status:** accepted

## Context

The engine, SQLite outbox, gateway supervisor, and host UI shell exist. The
engine can create an event, take seeds, lock the roster, generate a plan, start
the event, open a round, claim, submit, skip, expire an offer, and reveal. Tests
and `swap-sim` drive this sequence. The live application does not drive the
sequence. It cannot take a seed from Discord, open a round, or reveal an event.
The host UI has no lock, open-round, or reveal action.

## Decisions

1. **The host drives every round transition.** `start_event` changes the event
   state only. A separate `host_open_round` command calls `open_round`. The UI
   shows the next action. No timer opens a round. This protects the no-silent-
   auto-advance invariant.

2. **The roster lock is explicit.** `host_lock_roster` calls `lock_roster` and
   writes an audit entry. `host_generate_plan` refuses the signup state and
   requires a locked roster. A second lock returns a clear error.

3. **The current round is derived, not stored.** The current round is the
   highest opened round plus one. An event with no assignment is at round zero.
   No schema migration is necessary.

4. **Discord joins use a seed attachment.** `/swap join` sends instructions.
   A DM attachment from a user without a seed, while the event is in Signup,
   calls `signup` when needed and then `join_with_seed`. The display name is the
   Discord author name when available; otherwise the user id. The optional
   `/swap join` timezone and handoff options set the stored values. The
   defaults are UTC and four. Participant editing remains a later slice. A seed
   after the roster lock is refused and the user is offered the fill-in path in
   a later issue.

5. **The seed receipt is durable.** `join_with_seed` enqueues its receipt in the
   same transaction as the participant, track, and file rows. A crash cannot
   lose the receipt.

6. **The reveal is one transaction.** `reveal` sets the event state, computes
   credits, and enqueues the channel messages to the mapped reveal channel when
   a channel id is supplied. Long credit lists split at the Discord limit. The transaction rolls back if the reveal gate
   fails. The message contains track codes, seed owner names, and editor names
   because the reveal state permits identity. It contains no file hash.

7. **The gateway health is visible.** `GatewayHost` keeps the shared health
   snapshot. `host_bot_status` returns the gateway state, attempt, last error,
   connected time, and handler error count. The UI shows online, reconnecting,
   stopped, and failed states. The UI does not use color only.

8. **Inbound queues stop with the runtime.** Each participant queue has a stop
   signal and a join handle. `LiveRuntime::stop` stops the gateway, waits for
   in-flight events, and then drops the queues. A new runtime cannot process a
   newer message before an old in-flight submission commits.

## Invariants

- A seed never marks a seen pair.
- An offer never delivers a file. A claim delivers one file.
- A soft deadline never moves a track. This ADR adds no deadline behavior.
- A reveal never publishes file hashes.
- Pre-reveal participant surfaces never show seed owners or editor names.
- Files and absolute paths never cross IPC.
- The token never enters the UI, logs, SQLite, or git.
- UTC values are stored; the UI renders local time.

## Rollback

This change adds no schema migration. To roll back:

1. Stop the app. The gateway closes and the queues stop.
2. Check out the previous release commit.
3. Start the app on the same database. The previous schema version is unchanged.

The restart and backup/restore tests remain the recovery evidence.

## Verification

- `cargo test -p swap-engine --test classic_flow` covers the engine sequence.
- New tests cover seed-by-DM, explicit open-round, reveal effects, current-round
  projection, gateway health DTO, and queue stop.
- `scripts/check.sh` passes.
- UI screenshots at 1280×800 and 1024×700 cover Mission Control, Planner,
  Settings, and the bot-offline state. Three review waves have no must-fix.
