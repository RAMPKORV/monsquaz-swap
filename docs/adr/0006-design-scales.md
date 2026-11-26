# ADR 0006 — Lock the design scales

Date: 2026-11-26

## Status

Accepted for MSW-055. Tracker decision D-029.

## Context

The design gate started with locked core rules and advisory spacing, type, and
radius scales. The UI used values outside those scales, so the gate reported
154 advisory findings. An advisory scale cannot protect the interface from
slow drift. The host UI now fits the scales at every enforced viewport.

## Decisions

1. **The three scales are locked.** The spacing scale is 0, 2, 4, 6, 8, 12, 16,
   20, 24, 32, 40, 48, and 64 pixels. The type scale is 10, 10.5, 11, 11.5,
   12, 13, 14, 16, 18, 20, 24, 30, and 44 pixels. The radius scale is 0, 4, 6,
   8, 12, 16, and 999 pixels.
2. **The radius tokens follow the scale.** The tokens are 4, 6, and 8 pixels.
   The old extra-small token was removed because its nearest scale value is
   the same as the small token.
3. **Negative spacing is not in the scale.** A layout must not use a negative
   margin to align a control. Use zero and adjust the container.
4. **A scale change is a contract change.** It needs a tracker decision or an
   ADR, a policy diff, a baseline diff, and a gate run.
5. **A reviewed exception is debt.** It needs a baseline entry with a reason
   and an issue or an expiry. The baseline is not a mute button.

## Consequences

A new off-scale spacing, type, or radius value fails the design gate. The
existing UI has no scale findings and no baseline entries. Shadow tokens and
reduced motion stay advisory until a later cleanup.

## Evidence

- `design/policy.json` marks the three scales as locked.
- `design/baseline.json` has no entries.
- `scripts/design-check.sh` passes with 0 locked findings.
- The Refract matrix covers 26 pages at 1280x800, 1024x700, and 640x400.
