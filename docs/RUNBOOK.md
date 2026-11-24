# Host runbook

Use this file to run a real event. The app guides you, but this is the checklist.

## 1. Install

1. Download the Windows NSIS installer or the Linux `.deb`/AppImage from
   GitHub Releases.
2. Windows is unsigned. SmartScreen shows "Unknown publisher". Verify
   `SHA256SUMS` before you run the installer.
3. Open the app. Keep it running for the event.

## 2. Connect the bot

1. Create a Discord application and bot in the Developer Portal.
2. Copy the bot token.
3. Paste the token into the setup wizard. The app validates it and shows the
   bot name.
4. Invite the bot with the generated link. Use the minimum permissions.
5. Turn off "Public Bot" for a private host bot.
6. Map the announcement, chat, reveal, and help channels.
7. Send the test DM. Fix your own DM settings if it fails.

The app stores the token in the OS keychain. It never shows the token again.

## 3. Create the event

1. Choose a preset or start custom.
2. Set the mode: classic or async.
3. Set the length. Default: N-1 for classic; N-1 for async up to 12, then 6–8.
4. Attach the host pack. It is a module template with samples.
5. Set the allowlist. Default: modules, audio, zip.
6. Set deadlines, claim window, release time, and reminders.
7. Choose anonymity and reveal settings.
8. Read the warnings. Accept or change the plan.
9. Run the dry run.

## 4. Seed phase

1. Post the join announcement. Pin it.
2. Tell participants to enable DMs from server members.
3. Watch the DM risk list. Help anyone with blocked DMs.
4. Check the pack report for seeds that need review.
5. Remind non-seeders before the seed deadline.
6. Lock the roster. Unseeded signups become fill-ins by default.

## 5. Schedule

1. Generate the schedule.
2. Read the feasibility, overcrowding, group, and fun warnings.
3. Preview the schedule as a participant.
4. Edit only with a reason. Hard invariants are blocked.
5. Lock and commit. The bot posts the commitment.
6. Keep the commitment message. You need it at reveal.

## 6. Active event

### Every release window

1. Check the bot is online.
2. Check the pending offers and claims.
3. Let the two waves run.
4. Help anyone who did not receive the offer.

### Deadlines

1. The bot sends reminders and a decision card.
2. Decide: extend, skip, reassign, or wait.
3. Do not auto-advance. The host owns the decision.
4. Record the reason for an override.

### Skips and fill-ins

1. A skip returns the track unchanged.
2. If a fill-in is eligible, offer the slot.
3. If no fill-in is eligible, classic passes the track; async returns it to
   the pool.
4. Never repeat a track to the same person to rescue a slot.

### Bot offline

1. The app shows the outage.
2. Participants can still send DMs; the bot recovers them on reconnect.
3. Do not start a second bot session.
4. Extend deadlines by the outage duration if needed.

## 7. Reveal

1. Build the runbook in the app.
2. Open the presenter window. Turn on stream-safe mode.
3. Run the release stream. Show the lineage, play R1, then the final.
4. Finish the stream.
5. Press Reveal. The app publishes to the whole server.
6. Verify the credits, comments, schedule, and album links.
7. Export the ZIP and the personal keepsakes.
8. Send the album link and the runbook.

## 8. Incidents

| Incident | Action |
|---|---|
| Token leaked | Rotate the token. Stop the bot. Follow the security policy. |
| File is malicious | Quarantine it. Warn recipients. Do not distribute it. |
| Schedule is infeasible | Use the planner's repair. If you replan, record the reason. |
| Too many dropouts | Pause the event. Rebalance or merge groups. Use fill-ins. |
| Data loss | Restore a backup. Check the ledger. Do not restore a stale backup. |
| Participant harassment | Stop the event. Preserve evidence. Remove the person. |
| Accidental reveal | Stop. Tell the participants. Record what happened. Do not amplify. |

## 9. After the event

1. Publish the album and credits.
2. Export the archive.
3. Delete or retain data according to your privacy note.
4. Ask for feedback. Record what you would change.
5. Close the event.

## 10. Backups

- The app backs up before transitions and daily.
- Keep one copy outside the machine.
- Test a restore before the first real event.
- Backups expire after 90 days by default.
- Do not put the database in a cloud-synced folder.
