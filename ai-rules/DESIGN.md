# Design policy and the deterministic design gate

Read this file before you change a shared component, a layout, a token, or a
page-level style. The design gate makes the rules testable. The rules stay
adjustable, but a locked rule is a contract.

## Current status

- **Locked:** color tokens, no `!important`, WCAG AA text contrast, visible
  keyboard focus, the Refract overflow findings, and the spacing, type, and
  radius scales.
- **Advisory:** shadow tokens and reduced motion.
- The scales became locked in D-029 and ADR 0006 after the UI cleanup. A
  reviewed exception needs a baseline entry with a reason and an issue or an
  expiry.

Agents may change an advisory rule, fix code, or add a page to the matrix in a
normal pull request. Agents must not silently weaken a locked rule. A locked
rule change needs a tracker decision or an ADR, a policy diff, a baseline diff,
and a re-run of the gate.

## Source of truth

| File | Purpose |
|---|---|
| `design/policy.json` | Machine-readable rules and status. The gate reads this file. |
| `design/pages.json` | Page matrix, enforced viewports, and which pages get contrast and focus checks. |
| `design/baseline.json` | Reviewed existing debt. The gate fails on new or worse findings. |
| `design/refract.lock.json` | Pinned Refract commit. Refract is pre-release. |
| `ui/src/App.css` `:root` and `[data-theme="light"]` | Design tokens. Raw colors are allowed only here. |

## Principles

1. The host works at 1024×700 and at 200% zoom. The gate renders 1280×800,
   1024×700, and 640×400.
2. No horizontal overflow, no clipped element, and no hard-clipped text without
   an ellipsis at any enforced viewport.
3. Color comes from a token. A raw hex, `rgb()`, or `hsl()` value outside a
   token block is a locked violation.
4. Do not use `!important`. Fix the component or the selector.
5. Visible text meets WCAG AA: 4.5:1 for normal text, 3:1 for large text.
6. Every keyboard-focusable control shows a visible focus indicator.
7. Use the spacing, type, radius, and shadow scales. The spacing, type, and
   radius scales are locked. Shadow tokens are advisory. A reviewed exception
   needs a baseline entry with a reason and an issue or an expiry.
8. Respect `prefers-reduced-motion` for every animation or transition.
9. Empty, loading, error, long-content, and dense states are part of the
   design. They belong in the page matrix when the view can show them.
10. The host UI is a desktop tool. Do not add mobile-only patterns. The 640×400
    capture exists to prove zoom and reflow, not a phone layout.

## Commands

```bash
scripts/design-check.sh                 # full gate
scripts/design-check.sh --update-baseline --baseline-issue MSW-###
scripts/install-refract.sh              # build the pinned Refract CLI only
```

`scripts/check.sh` runs the design gate. CI runs it in the `design` job. The
worktree finish check runs it through `scripts/check.sh`.

## Baseline policy

The baseline is reviewed debt, not a mute button.

- Every entry has a `reason` and either an `issue` or an `expires` date.
- `--update-baseline` preserves existing reasons and issues. New entries use
  the `--baseline-issue` value. If it is `UNASSIGNED`, the next check fails.
- An expired entry fails while the finding still exists.
- Remove an entry when the finding is fixed. Do not lower a count to make a
  regression pass.
- Review the baseline diff like code. A large unexplained baseline increase is
  a design regression.

## Changing a rule

1. Edit `design/policy.json`. Keep the description and rationale current.
2. If the rule controls a new finding type, add it to `scripts/design-check.mjs`.
3. Run `scripts/design-check.sh`. Expect new failures if the rule is stricter.
4. Fix the code, or update the baseline with a reason, issue, and expiry.
5. For a locked rule, add a tracker decision or ADR. Link it in the pull
   request. The policy diff and the baseline diff show the contract change.

## Refract

Refract is pinned by commit because it is pre-release. The install script
builds it into `.cache/refract/<commit>` and installs Chromium through
Playwright. CI caches that directory. To update Refract, change the commit in
`design/refract.lock.json`, update the cache key in `.github/workflows/ci.yml`,
run the gate, and review any new findings. Do not commit the cache or
`design/artifacts/`.

## False positives

Prefer a more precise selector or rule over an allowlist entry. If the finding
is real but accepted for now, add a baseline entry with a reason, an issue, and
an expiry. The expiry is the review date. Do not leave an entry without both an
issue and an expiry.
