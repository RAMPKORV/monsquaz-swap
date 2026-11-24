# UX and copy rules

The vibe is playful retro Monsquaz, but the job is serious: people need to know
what to do, when, and what is private. Clarity wins over cleverness.

## Voice

- Warm, a little irreverent, never mean. No public shaming for lateness or
  skipping.
- Every instruction follows ASD-STE100 (`ai-rules/STE.md`): short, active, one
  action per sentence. The playful voice is allowed around the instruction.
- Short sentences. One primary action per message.
- Use the event's language: seed, track, handoff, claim, skip, fill-in, reveal.
- Never imply perfect anonymity. Say: "Anonymous from other participants until
  the reveal. The host can see the mapping."
- Do not use ASCII art in critical instructions; it breaks screen readers.
- Localize dates/times; show UTC when precision matters.

## Discord copy patterns

**Offer (DM):**
> Track `S7F3A` is yours if you want it.
> Claim within 12h; the deadline starts when you claim.
> [Claim] [Not now]

**After claim:**
> Claimed. `S7F3A.it` is attached. Due <t:...:F> (<t:...:R>).
> [Submit] [Skip] [Need more time]

**Reminder:**
> 6h left on `S7F3A`. [Still working] [Need +6h] [Skip]
> Quiet hours respected; this is the only nudge before the deadline.

**Deadline passed:**
> The deadline passed on `S7F3A`. The host has been notified.
> [Finishing today] [Need an extension] [Skip]

**Skip after claim:**
> Skipped. The track goes back unchanged, and you will not receive it again.
> Your past credits are kept. You can leave the event now if you need to.

**No eligible fill-in / pass-through:**
> No one could take this turn. The track moves on unchanged. This is not a
> failure; the chain continues.

**Reveal:**
> The album is out. Here is who touched what, in order. Thank you for making
> something none of us could have made alone.

Rules for messages:
- Never include another participant's name in a private offer/submission DM.
- Use Discord timestamps for local time; avoid hard-coded timezone names.
- Buttons for the primary action; never require a typed command to submit.
- Aggregate public progress only ("Round 3 underway · next release in 6h").
- A quiet hours window is respected for non-urgent DMs; deadline prompts are
  scheduled earlier, not at 3am.

## Host app

- **Mission control** shows one next action, the event state, bot health, at-risk
  participants, and pending decisions. Avoid a wall of metrics.
- **Warnings** say what will happen, why, and the fix. Soft warnings for fun,
  size, overcrowding; hard blocks only for invariants.
- **Destructive actions** (reveal, replan, delete, restore) require a typed
  confirmation or a two-step dialog with consequences.
- **Status** uses text + icon + color, never color alone.
- **Stream-safe mode** is prominent before the reveal stream; it hides mapping
  and redacts IDs.
- **Empty states** explain the next step; do not show a blank table.
- **Errors** say what happened, what to do, and a copyable code. No stack traces,
  paths, tokens, or raw Discord payloads.
- **Accessibility:** WCAG AA contrast, keyboard navigation, focus rings, reduced
  motion, semantic headings, screen-reader labels, 200% zoom.
- **Theme:** dark playful default, light theme; no external fonts required.

## Fun without pressure

- A host-only fun score is a planning aid. Do not show it to participants as a
  grade.
- Post-event stats celebrate effort (edits, rescues, longest chain) without
  ranking people.
- Fill-ins get credit, not a "rescue" label in public.
- Skips are private by default; the reveal shows anonymous gaps unless the host
  chooses otherwise.
- The reveal is a show: lineage, R1 vs final, credits, a big thank-you. Keep it
  generous.

## Copy checklist

- Does the user know the next action in one read?
- Is the deadline in their local time?
- Is anything that should be private visible?
- Is a skip/leave framed without shame?
- Does an error tell them how to fix it?
- Would a first-time participant understand "claim" vs "submit"?
