import { useEffect, useRef, useState } from "react";
import type { EventDraft, SetupData } from "../types";
import { Button, Field, PageHeader, Skeleton, StatusPill, Toggle } from "../components";
import { Icon } from "../icons";
import { JoinInstructions } from "./JoinInstructions";

const steps = [
  { id: "basics", label: "Basics", detail: "Name, mode, windows" },
  { id: "files", label: "Pack and files", detail: "Allowlist and review" },
  { id: "groups", label: "Groups", detail: "Rotation and cadence" },
  { id: "review", label: "Review", detail: "Warnings and create" },
];

const intros = [
  "Set the name, mode, and deadline windows.",
  "Choose what people can submit and how the pack is reviewed.",
  "Decide how people rotate for a large classic event.",
  "Check the shape, then create the event.",
];

const extensions = ["it", "mptm", "xm", "mod", "s3m", "mtm", "669", "wav", "flac", "mp3", "ogg", "zip"];

export function SetupView({
  data,
  timezone,
  onCreate,
}: {
  data: SetupData;
  timezone?: string;
  onCreate: (draft: EventDraft) => void;
}) {
  const [step, setStep] = useState(() => {
    const initial = new URLSearchParams(window.location.search).get("step");
    if (initial === "review") {
      return 3;
    }
    const numeric = Number(initial);
    return Number.isInteger(numeric) && numeric >= 0 && numeric <= 3
      ? numeric
      : 0;
  });
  const titleRef = useRef<HTMLHeadingElement | null>(null);
  const firstStep = useRef(true);

  useEffect(() => {
    if (firstStep.current) {
      firstStep.current = false;
      return;
    }
    titleRef.current?.focus();
  }, [step]);
  const [draft, setDraft] = useState<EventDraft>(data.draft);

  function update<K extends keyof EventDraft>(key: K, value: EventDraft[K]) {
    setDraft({ ...draft, [key]: value });
  }

  const canContinue = step !== 0 || draft.name.trim().length > 0;
  const isLast = step === steps.length - 1;

  return (
    <div className="view">
      <PageHeader
        title="Event setup"
        description={
          data.firstRun
            ? "Set the shape before anyone seeds a track. These choices lock when you start the event."
            : "Set the shape for a new event. The current event is unchanged. These choices lock when you start the event."
        }
        actions={
          <StatusPill tone={data.firstRun ? "muted" : "info"}>
            New event draft
          </StatusPill>
        }
      />

      <ol className="wizard-steps" aria-label="Setup steps" role="list">
        {steps.map((entry, index) => (
          <li key={entry.id}>
            <button
              type="button"
              className={`wizard-step ${
                index === step ? "wizard-step--active" : ""
              } ${index < step ? "wizard-step--done" : ""}`}
              aria-current={index === step ? "step" : undefined}
              onClick={() => setStep(index)}
            >
              <span className="wizard-step__number">
                {index < step ? <Icon name="check" size={14} /> : index + 1}
              </span>
              <span>
                {index < step ? (
                  <span className="visually-hidden">Completed </span>
                ) : null}
                <strong>{entry.label}</strong>
                <small>{entry.detail}</small>
              </span>
            </button>
          </li>
        ))}
      </ol>

      <section className="card wizard-card" aria-labelledby="wizard-title">
        <div className="card__header">
          <div>
            <h2 id="wizard-title" ref={titleRef} tabIndex={-1}>
              {steps[step]?.label}
            </h2>
            <p className="card__subtitle">{intros[step]}</p>
          </div>

        </div>

        {step === 0 ? <BasicsStep draft={draft} update={update} /> : null}
        {step === 1 ? <FilesStep draft={draft} update={update} /> : null}
        {step === 2 ? <GroupsStep draft={draft} update={update} /> : null}
        {step === 3 ? (
          <ReviewStep data={data} draft={draft} timezone={timezone} />
        ) : null}

        <div className="wizard-actions">
          <Button
            variant="secondary"
            onClick={() => setStep(Math.max(0, step - 1))}
            disabled={step === 0}
          >
            Back
          </Button>
          {isLast ? (
            <Button
              variant="primary"
              icon="check"
              onClick={() => onCreate(draft)}
              disabled={draft.name.trim().length === 0}
            >
              Create event
            </Button>
          ) : (
            <Button
              variant="primary"
              icon="arrow-right"
              onClick={() => setStep(step + 1)}
              disabled={!canContinue}
            >
              Continue
            </Button>
          )}
        </div>
      </section>

    </div>
  );
}

function BasicsStep({
  draft,
  update,
}: {
  draft: EventDraft;
  update: <K extends keyof EventDraft>(key: K, value: EventDraft[K]) => void;
}) {
  return (
    <div className="wizard-grid">
      <Field
        label="Event name"
        htmlFor="event-name"
        helpId="event-name-help"
        help="The name the host sees. Participants see track codes, not this name."
      >
        <input
          id="event-name"
          aria-describedby="event-name-help"
          className="input"
          value={draft.name}
          placeholder="Night Shift Vol. 2"
          onChange={(event) => update("name", event.target.value)}
        />
      </Field>

      <fieldset className="choice-field">
        <legend className="field-label">Mode</legend>
        <div className="choice-row" role="radiogroup" aria-label="Event mode">
          {(["Classic", "Async"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={draft.mode === mode}
              className={`choice-card ${draft.mode === mode ? "choice-card--active" : ""}`}
              onClick={() => update("mode", mode)}
            >
              <Icon name={mode === "Classic" ? "shuffle" : "calendar"} size={20} />
              <span>
                <strong>{mode}</strong>
                <small>
                  {mode === "Classic"
                    ? "Rounds, groups, and one track per person each round."
                    : "A shared queue. People claim a track when they are ready."}
                </small>
              </span>
              {draft.mode === mode ? <Icon name="check" size={16} /> : null}
            </button>
          ))}
        </div>
      </fieldset>

      <div className="form-grid">
        <Field
          label="Exchange rounds (R2..RN)"
          htmlFor="rounds"
          helpId="rounds-help"
          help="Classic events have one seed round plus one exchange round for each other track. Leave empty to use the seeded track count."
        >
          <input
            id="rounds"
            aria-describedby="rounds-help"
            className="input"
            type="number"
            min={1}
            max={40}
            value={draft.rounds ?? ""}
            placeholder="Automatic"
            onChange={(event) =>
              update("rounds", event.target.value ? Number(event.target.value) : null)
            }
          />
        </Field>
        <Field
          label="Schedule visibility"
          htmlFor="visibility"
          helpId="visibility-help"
          help={
            draft.scheduleVisibility === "Hidden"
              ? "Participants see only their own assignments. Group names and the seed band stay hidden."
              : "Warning: the open schedule shows the full matrix, seed owners, editor names, titles, and comments. The event has no anonymity."
          }
        >
          <select
            id="visibility"
            aria-describedby="visibility-help"
            className="input"
            value={draft.scheduleVisibility}
            onChange={(event) =>
              update(
                "scheduleVisibility",
                event.target.value as EventDraft["scheduleVisibility"],
              )
            }
          >
            <option value="Hidden">Own assignments only (recommended)</option>
            <option value="Open">Open schedule (no anonymity)</option>
          </select>
        </Field>
        <Field
          label="Participation"
          htmlFor="handoff-policy"
          helpId="handoff-policy-help"
          help={
            draft.handoffPolicy === "Strict"
              ? "Full coverage. Every active person must set a handoff cap at least equal to the exchange rounds. A lower cap blocks the schedule."
              : "Cap-limited. Each person edits their earliest exchange rounds up to their cap. Empty cells become coverage gaps."
          }
        >
          <select
            id="handoff-policy"
            aria-describedby="handoff-policy-help"
            className="input"
            value={draft.handoffPolicy}
            onChange={(event) =>
              update(
                "handoffPolicy",
                event.target.value as EventDraft["handoffPolicy"],
              )
            }
          >
            <option value="Strict">Full coverage (strict)</option>
            <option value="Flexible">Cap-limited (flexible)</option>
          </select>
        </Field>
      </div>
      <details className="advanced-settings">
        <summary>
          <Icon name="chevron-down" size={16} /> Advanced timing
        </summary>
        <p className="field-help">
          Keep the defaults for a normal event. Change these only when the group
          needs a different rhythm.
        </p>
        <div className="form-grid">
        <Field
          label="Claim window"
          htmlFor="claim-window"
        helpId="claim-help"
          help="Time to accept an offer, in hours, before the track returns to the pool."
        >
          <div className="number-control">
            <input
              id="claim-window"
          aria-describedby="claim-help"
              className="input"
              type="number"
              min={1}
              max={48}
              value={draft.claimWindowHours}
              onChange={(event) => update("claimWindowHours", Number(event.target.value) || 1)}
            />
            <span>hours</span>
          </div>
        </Field>
        <Field
          label="Round window"
          htmlFor="round-window"
        helpId="round-help"
          help="Hours to edit and submit after the person accepts."
        >
          <div className="number-control">
            <input
              id="round-window"
          aria-describedby="round-help"
              className="input"
              type="number"
              min={12}
              max={336}
              value={draft.roundWindowHours}
              onChange={(event) => update("roundWindowHours", Number(event.target.value) || 12)}
            />
            <span>hours</span>
          </div>
        </Field>
        <Field
          label="Quiet hours start"
          htmlFor="quiet-start"
          helpId="quiet-start-help"
          help="Reminders wait during local quiet hours. Use the same start and end hour to turn quiet hours off."
        >
          <div className="number-control">
            <input
              id="quiet-start"
              aria-describedby="quiet-start-help"
              className="input"
              type="number"
              min={0}
              max={23}
              value={draft.quietHoursStart}
              onChange={(event) =>
                update("quietHoursStart", Number(event.target.value) || 0)
              }
            />
            <span>hours</span>
          </div>
        </Field>
        <Field
          label="Quiet hours end"
          htmlFor="quiet-end"
          helpId="quiet-end-help"
          help="The first local hour when reminders can arrive."
        >
          <div className="number-control">
            <input
              id="quiet-end"
              aria-describedby="quiet-end-help"
              className="input"
              type="number"
              min={0}
              max={23}
              value={draft.quietHoursEnd}
              onChange={(event) =>
                update("quietHoursEnd", Number(event.target.value) || 0)
              }
            />
            <span>hours</span>
          </div>
        </Field>
        </div>
      </details>
    </div>
  );
}

function FilesStep({
  draft,
  update,
}: {
  draft: EventDraft;
  update: <K extends keyof EventDraft>(key: K, value: EventDraft[K]) => void;
}) {
  function toggleExtension(extension: string) {
    update(
      "allowlist",
      draft.allowlist.includes(extension)
        ? draft.allowlist.filter((value) => value !== extension)
        : [...draft.allowlist, extension],
    );
  }

  return (
    <div className="wizard-grid">
      <div className="form-grid">
        <Field label="Host pack" htmlFor="pack-type" help="The module template and samples people start from." helpId="pack-help">
          <select
            id="pack-type"
          aria-describedby="pack-help"
            className="input"
            value={draft.packType}
            onChange={(event) =>
              update("packType", event.target.value as EventDraft["packType"])
            }
          >
            <option>Module template + samples</option>
            <option>Samples only</option>
            <option>Reference track</option>
            <option>None</option>
          </select>
        </Field>
        <div className="field">
          <span className="field-label">Pack review</span>
          <Toggle
            checked={draft.packRequired}
            onChange={(next) => update("packRequired", next)}
            label="Review unknown samples"
            description="Flag new samples and instruments for the host. Never reject automatically."
          />
        </div>
      </div>

      <fieldset className="allowlist-field">
        <legend className="field-label">Allowed file types</legend>
        <div className="chip-row">
          {extensions.map((extension) => {
            const active = draft.allowlist.includes(extension);
            return (
              <button
                key={extension}
                type="button"
                className={`chip ${active ? "chip--active" : ""}`}
                aria-pressed={active}
                onClick={() => toggleExtension(extension)}
              >
                .{extension}
              </button>
            );
          })}
        </div>
        <p className="field-help">
          Default is modules, audio, and zip. Format changes are refused; the
          original format must stay.
        </p>
      </fieldset>

      <div className="fact-row">
        <span>
          <Icon name="shield" size={18} />
          Metadata scrub and A/B render
        </span>
        <span className="card__meta">20 MiB cap</span>
      </div>
    </div>
  );
}

function GroupsStep({
  draft,
  update,
}: {
  draft: EventDraft;
  update: <K extends keyof EventDraft>(key: K, value: EventDraft[K]) => void;
}) {
  return (
    <div className="wizard-grid">
      <div className="form-grid">
        <Field
          label="Group suggestion"
          htmlFor="group-mode"
          helpId="group-mode-help"
          help={
            draft.groupMode === "Manual"
              ? "Set the group count and size yourself. Soft bounds are 5 to 12 people."
              : draft.groupMode === "Single group"
                ? "Single group uses the full roster. Choose Manual to override the shape."
                : "The planner sets the group count and size from the locked roster. Choose Manual to override."
          }
        >
          <select
            id="group-mode"
            aria-describedby="group-mode-help"
            className="input"
            value={draft.groupMode}
            onChange={(event) =>
              update("groupMode", event.target.value as EventDraft["groupMode"])
            }
          >
            <option value="Lookup">From the locked roster</option>
            <option value="Single group">Single group</option>
            <option value="Manual">Manual</option>
          </select>
        </Field>
        <Field
          label="Rotation"
          htmlFor="rotation"
        helpId="rotation-help"
          help="What moves when a group phase ends."
        >
          <select
            id="rotation"
          aria-describedby="rotation-help"
            className="input"
            value={draft.rotation}
            onChange={(event) =>
              update("rotation", event.target.value as EventDraft["rotation"])
            }
          >
            <option>Participants</option>
            <option>Track pools</option>
            <option>Both</option>
          </select>
        </Field>
        <Field
          label="Groups"
          htmlFor="group-count"
          help={
            draft.groupMode === "Manual"
              ? "Between 1 and 4 groups."
              : draft.groupMode === "Single group"
                ? "Fixed at one group."
                : "Set from the locked roster."
          }
          helpId="group-count-help"
        >
          <input
            id="group-count"
            aria-describedby="group-count-help"
            className="input"
            type="number"
            min={1}
            max={4}
            disabled={draft.groupMode !== "Manual"}
            value={draft.groupMode === "Manual" ? draft.groupCount : ""}
            placeholder={draft.groupMode === "Single group" ? "1" : "From roster"}
            onChange={(event) => update("groupCount", Number(event.target.value) || 1)}
          />
        </Field>
        <Field
          label="People per group"
          htmlFor="group-size"
          helpId="group-size-help"
          help={
            draft.groupMode === "Manual"
              ? "Between 2 and 20 people. Soft bounds are 5 to 12."
              : draft.groupMode === "Single group"
                ? "The full roster."
                : "Set from the locked roster."
          }
        >
          <input
            id="group-size"
            aria-describedby="group-size-help"
            className="input"
            type="number"
            min={2}
            max={20}
            disabled={draft.groupMode !== "Manual"}
            value={draft.groupMode === "Manual" ? draft.groupSize : ""}
            placeholder={
              draft.groupMode === "Single group" ? "All people" : "From roster"
            }
            onChange={(event) => update("groupSize", Number(event.target.value) || 2)}
          />
        </Field>
        <Field
          label="Swap cadence"
          htmlFor="cadence"
        helpId="cadence-help"
          help="When groups rotate. Every K rounds is the default."
        >
          <select
            id="cadence"
          aria-describedby="cadence-help"
            className="input"
            value={draft.swapCadence}
            onChange={(event) =>
              update("swapCadence", event.target.value as EventDraft["swapCadence"])
            }
          >
            <option value="K rounds">Every K rounds</option>
            <option value="Calendar">On a calendar date</option>
            <option value="Host-triggered">When the host triggers it</option>
            <option value="Exhausted">When a group exhausts</option>
          </select>
        </Field>
      </div>

      <div className="note-card">
        <span className="note-card__icon">
          <Icon name="users" size={20} />
        </span>
        <div>
          <h3>Groups are hidden from participants</h3>
          <p>
            Participants see their current assignment and their own season.
            They never see a group name, another group, or the fun score.
          </p>
        </div>
      </div>

      {draft.groupMode === "Manual" && draft.groupSize < 5 ? (
        <p className="inline-warning">
          <Icon name="alert" size={16} /> A group of {draft.groupSize} is below
          the soft minimum of 5. The planner warns but does not block the plan.
        </p>
      ) : null}
      {draft.groupMode === "Manual" && draft.groupSize > 12 ? (
        <p className="inline-warning">
          <Icon name="alert" size={16} /> A group of {draft.groupSize} is above
          the soft maximum of 12. The planner warns but does not block the plan.
        </p>
      ) : null}
    </div>
  );
}

function ReviewStep({
  data,
  draft,
  timezone,
}: {
  data: SetupData;
  draft: EventDraft;
  timezone?: string;
}) {
  return (
    <div className="wizard-grid">
      <dl className="review-grid">
        <div>
          <dt>Name</dt>
          <dd>{draft.name || "No name yet"}</dd>
        </div>
        <div>
          <dt>Mode</dt>
          <dd>{draft.mode}</dd>
        </div>
        <div>
          <dt>Rounds</dt>
          <dd>
            {draft.rounds
              ? `R1 seed + ${draft.rounds} exchanges = ${draft.rounds + 1} total`
              : "Automatic: R1 seed + N-1 exchanges = N total"}
          </dd>
        </div>
        <div>
          <dt>{draft.mode === "Async" ? "Claim window" : "Round deadline"}</dt>
          <dd>
            {draft.mode === "Async"
              ? `${draft.claimWindowHours} hours`
              : `${draft.roundWindowHours} hours (final 48 hours)`}
          </dd>
        </div>
        {draft.mode === "Async" ? (
          <div>
            <dt>Edit window</dt>
            <dd>{draft.roundWindowHours} hours</dd>
          </div>
        ) : null}
        <div>
          <dt>Round window</dt>
          <dd>{draft.roundWindowHours} hours</dd>
        </div>
        <div>
          <dt>Groups</dt>
          <dd>
            {draft.groupMode === "Lookup"
              ? "From the locked roster"
              : draft.groupMode === "Single group"
                ? "Single group"
                : `${draft.groupCount} × ${draft.groupSize}`}
          </dd>
        </div>
        <div>
          <dt>Participation</dt>
          <dd>
            {draft.handoffPolicy === "Strict"
              ? "Full coverage (strict)"
              : "Cap-limited (flexible)"}
          </dd>
        </div>
        <div>
          <dt>Schedule</dt>
          <dd>
            {draft.scheduleVisibility === "Open"
              ? "Open (no anonymity)"
              : "Private (own assignments)"}
          </dd>
        </div>
        <div>
          <dt>Pack</dt>
          <dd>{draft.packType}</dd>
        </div>
        <div>
          <dt>Allowed types</dt>
          <dd>{draft.allowlist.length} selected</dd>
        </div>
      </dl>

      <div className="warnings">
        {data.warnings.map((warning) => (
          <div className={`warning warning--${warning.tone}`} key={warning.id}>
            <Icon
              name={
                warning.tone === "danger"
                  ? "alert"
                  : warning.tone === "warn"
                    ? "clock"
                    : "check"
              }
              size={18}
            />
            <div>
              <span className="visually-hidden">
                {warning.tone === "danger"
                  ? "Danger: "
                  : warning.tone === "warn"
                    ? "Warning: "
                    : "Note: "}
              </span>
              <strong>{warning.title}</strong>
              <p>{warning.detail}</p>
            </div>
          </div>
        ))}
      </div>

      <JoinInstructions
        formats={draft.allowlist}
        maxMib={20}
        exampleTimezone={timezone}
        preview
        headingLevel={3}
        openSchedule={draft.scheduleVisibility === "Open"}
        exchangeRounds={draft.rounds ?? 0}
        handoffPolicy={draft.handoffPolicy}
      />
    </div>
  );
}

export function SetupSkeleton() {
  return (
    <div className="view" role="status" aria-busy="true" aria-label="Loading event setup">
      <span className="visually-hidden">Loading event setup</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="100px" height={12} />
          <Skeleton width="420px" height={44} className="skeleton--title" />
          <Skeleton width="min(100%, 560px)" height={16} />
        </div>
      </div>
      <div className="wizard-steps">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton key={index} width="100%" height={64} />
        ))}
      </div>
      <section className="card wizard-card">
        <Skeleton width="180px" height={24} />
        <div className="skeleton-stack">
          <Skeleton width="100%" height={44} />
          <Skeleton width="100%" height={44} />
          <Skeleton width="70%" height={44} />
        </div>
      </section>
    </div>
  );
}
