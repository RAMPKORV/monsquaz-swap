import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Decision, MissionControl, PackInfo } from "../types";
import {
  Button,
  Dialog,
  EmptyState,
  Field,
  Meter,
  PageHeader,
  Skeleton,
  StatusPill,
} from "../components";
import { Icon } from "../icons";
import { JoinInstructions } from "./JoinInstructions";

function stageTone(stage: string) {
  if (stage === "Active") return "ok" as const;
  if (stage === "Paused") return "warn" as const;
  if (stage === "Reveal") return "info" as const;
  return "muted" as const;
}

export function MissionControlView({
  data,
  timezone,
  onAction,
  onDecision,
  onNavigate,
  onUploadPack,
  onOpenSeedRound,
  onCloseSeedRound,
  onExtendSeedDeadline,
  onDecideLateSeed,
  onMoveRoundDeadline,
}: {
  data: MissionControl;
  timezone?: string;
  onAction: (label: string) => void;
  onDecision: (decision: Decision, action: string) => void;
  onNavigate: (view: "audit" | "settings") => void;
  onUploadPack: (path: string) => Promise<PackInfo>;
  onOpenSeedRound: (deadlineAt: number, channelId: string | null) => Promise<void>;
  onCloseSeedRound: () => Promise<void>;
  onExtendSeedDeadline: (deadlineAt: number) => Promise<void>;
  onDecideLateSeed: (id: string, accept: boolean) => Promise<void>;
  onMoveRoundDeadline: (
    roundNo: number,
    deadlineAt: number,
    reason: string,
  ) => Promise<void>;
}) {
  const initialDialog = new URLSearchParams(window.location.search).get("dialog");
  const [confirmReveal, setConfirmReveal] = useState(initialDialog === "reveal");
  const [confirmLock, setConfirmLock] = useState(initialDialog === "lock");
  const [moveDeadline, setMoveDeadline] = useState<string | null>(null);
  const [moveReason, setMoveReason] = useState("");
  const [selectedDecision, setSelectedDecision] = useState<Decision | null>(
    initialDialog === "decision" ? (data.decisions[0] ?? null) : null,
  );
  const [confirmMiss, setConfirmMiss] = useState(
    initialDialog === "decision" &&
      new URLSearchParams(window.location.search).get("confirm") === "missed",
  );
  if (data.firstRun || !data.event) {
    return <FirstRun onAction={onAction} />;
  }

  const event = data.event;
  const botTone =
    data.bot.status === "Online"
      ? "ok"
      : data.bot.status === "Connecting" || data.bot.status === "Reconnecting"
        ? "warn"
        : data.bot.status === "Offline"
          ? "danger"
          : "muted";

  return (
    <div className="view">
      <PageHeader
        title={event.name}
        description={`${
          event.mode === "Classic" ? "Classic relay" : "Async queue"
        }, host view.`}
        actions={
          <div className="header-actions">
            <StatusPill tone={stageTone(event.stage)}>
              {event.stage}
            </StatusPill>
            <StatusPill tone={botTone}>{data.bot.status}</StatusPill>
          </div>
        }
      />

      {data.bot.status === "Offline" ? (
        <div className="banner banner--danger" role="status">
          <Icon name="alert" size={20} />
          <div>
            <strong>Bot offline.</strong> Offers, reminders, and receipts wait in
            the local queue. A file sent while the bot is offline is not
            guaranteed to arrive. Ask people to send it again after the bot
            reconnects.
          </div>
        </div>
      ) : data.bot.status === "Connecting" || data.bot.status === "Reconnecting" ? (
        <div className="banner banner--warn" role="status">
          <Icon name="clock" size={20} />
          <div>
            <strong>
              {data.bot.status === "Connecting"
                ? "Bot connecting."
                : "Bot reconnecting."}
            </strong>{" "}
            {data.bot.detail} Offers, reminders, and receipts wait in the local
            queue. A file sent now may need a resend.
          </div>
        </div>
      ) : null}

      <p
        className={`privacy-line${
          event.openSchedule ? " privacy-line--warn" : ""
        }`}
      >
        <Icon name={event.openSchedule ? "eye-off" : "shield"} size={16} />
        {event.openSchedule
          ? "Open schedule is on. Everyone can see the matrix, owners, names, titles, and comments. The event has no anonymity."
          : "Participants are anonymous to each other until the reveal. You can see the mapping."}
      </p>

      {event.stage === "Setup" || event.stage === "Join phase" ? (
        <SeedRoundCard
          event={event}
          lateSeeds={data.lateSeeds}
          timezone={timezone}
          onUploadPack={onUploadPack}
          onOpenSeedRound={onOpenSeedRound}
          onCloseSeedRound={onCloseSeedRound}
          onExtendSeedDeadline={onExtendSeedDeadline}
          onDecideLateSeed={onDecideLateSeed}
        />
      ) : null}

      {event.stage === "Join phase" ? (
        <JoinInstructions
          formats={data.join.formats}
          maxMib={data.join.maxFileMib}
          exampleTimezone={timezone}
          openSchedule={event.openSchedule}
          showOpenBanner={false}
          exchangeRounds={event.exchangeRounds}
          handoffPolicy={event.handoffPolicy}
        />
      ) : null}

      {data.nextAction && event.stage !== "Setup" ? (
        <section
          className={`next-action next-action--${data.nextAction.tone}`}
          aria-labelledby="next-action-title"
        >
          <div className="next-action__copy">
            <h2 id="next-action-title">{data.nextAction.title}</h2>
            <p>{data.nextAction.detail}</p>
          </div>
          <div className="next-action__actions">
            <Button
              variant="primary"
              icon="arrow-right"
              onClick={() => {
                if (data.nextAction?.actionLabel === "Reveal event") {
                  setConfirmReveal(true);
                  return;
                }
                if (data.nextAction?.actionLabel === "Lock roster") {
                  setConfirmLock(true);
                  return;
                }
                onAction(data.nextAction?.actionLabel ?? "");
              }}
            >
              {data.nextAction.actionLabel}
            </Button>
            {data.nextAction.secondaryLabel &&
            !/^pause/i.test(data.nextAction.secondaryLabel) ? (
              <Button
                variant="quiet"
                icon={
                  /^pause/i.test(data.nextAction.secondaryLabel)
                    ? "pause"
                    : "arrow-right"
                }
                onClick={() => onAction(data.nextAction?.secondaryLabel ?? "")}
              >
                {data.nextAction.secondaryLabel}
              </Button>
            ) : null}
          </div>
        </section>
      ) : null}

      <div className="mission-grid">
        <div className="mission-main">
          <section className="card" aria-labelledby="round-progress-title">
            <div className="card__header">
              <div>
                <h2 id="round-progress-title">Round progress</h2>
                <p className="card__subtitle">
                  {event.round === 0
                    ? "The seed round is not open."
                    : event.seedRoundOpen
                      ? "Round 1 is the seed round. Each person makes one track from the host pack."
                      : `${event.rounds - event.round} rounds remain after this one.`}
                </p>
              </div>
              <div className="header-actions">
                <span className="card__meta">
                  {event.round === 0
                    ? "Not started"
                    : event.seedRoundOpen
                      ? `Seed round · R1 of ${event.rounds}`
                      : `Round ${event.round} of ${event.rounds}`}
                </span>
                {event.stage === "Active" && event.round >= 2 ? (
                  <Button
                    variant="quiet"
                    icon="clock"
                    onClick={() => {
                      const next = new Date(Date.now() + 24 * 60 * 60 * 1000);
                      const offset = next.getTimezoneOffset() * 60_000;
                      setMoveDeadline(
                        new Date(next.getTime() - offset)
                          .toISOString()
                          .slice(0, 16),
                      );
                    }}
                  >
                    Move deadline
                  </Button>
                ) : null}
              </div>
            </div>
            <Meter
              value={event.round}
              max={event.rounds}
              label="Classic round progress"
            />
            <ol
              className="timeline"
              style={
                {
                  "--timeline-steps": Math.max(1, data.timeline.length),
                } as CSSProperties
              }
            >
              {data.timeline.map((step) => (
                <li
                  key={step.id}
                  className={`timeline__step timeline__step--${step.state}`}
                  aria-current={step.state === "current" ? "step" : undefined}
                >
                  <span className="timeline__dot" aria-hidden="true" />
                  <span className="timeline__label">
                    <span className="visually-hidden">
                      {step.state === "done"
                        ? "Done: "
                        : step.state === "current"
                          ? "Current: "
                          : "Next: "}
                    </span>
                    {step.label}
                  </span>
                  <span className="timeline__caption">{step.caption}</span>
                </li>
              ))}
            </ol>
          </section>

          <section className="card" aria-labelledby="decisions-title">
            <div className="card__header">
              <div>
                <h2 id="decisions-title" tabIndex={-1}>
                  Decisions waiting
                </h2>
                <p className="card__subtitle">
                  Nothing moves until you choose. No silent auto-advance.
                </p>
              </div>
              <StatusPill tone={data.decisions.length > 0 ? "warn" : "ok"}>
                {data.decisions.length} open
              </StatusPill>
            </div>
            {data.decisions.length === 0 ? (
              <EmptyState
                icon="check"
                title="No decisions waiting"
                detail={
                  event.stage === "Active" &&
                  event.round > 0 &&
                  event.round >= event.rounds
                    ? "Every round is complete. Reveal when the stream ends."
                    : "The round is healthy. Reminders and deadlines run on their own."
                }
              />
            ) : (
              <ul className="decision-list">
                {data.decisions.map((decision) => (
                  <li className="decision" key={decision.id}>
                    <div
                      className={`decision__kind ${
                        decision.kind === "Schedule"
                          ? "decision__kind--info"
                          : ""
                      }`}
                    >
                      <Icon
                        name={decision.kind === "Schedule" ? "calendar" : "alert"}
                        size={16}
                      />
                      <span>{decision.kind}</span>
                    </div>
                    <div className="decision__copy">
                      <h3>{decision.title}</h3>
                      <p>{decision.detail}</p>
                    </div>
                    <div className="decision__side">
                      <span className="decision__due">{decision.due}</span>
                      {decision.kind === "Schedule" ? (
                        <Button
                          variant="secondary"
                          ariaLabel={`${decision.actionLabel}: ${decision.title}`}
                          onClick={() => onAction("Open schedule planner")}
                        >
                          {decision.actionLabel}
                        </Button>
                      ) : decision.kind === "Deadline" ||
                        decision.kind === "Extension" ? (
                        <Button
                          variant="secondary"
                          ariaLabel={`${decision.actionLabel}: ${decision.title}`}
                          onClick={() => {
                            setSelectedDecision(decision);
                            setConfirmMiss(false);
                          }}
                        >
                          {decision.actionLabel}
                        </Button>
                      ) : (
                        <StatusPill tone="muted">Soon</StatusPill>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="card" aria-labelledby="activity-title">
            <div className="card__header">
              <div>
                <h2 id="activity-title">Recent activity</h2>
                <p className="card__subtitle">
                  Host-only view. Participants never see these events.
                </p>
              </div>
              <Button variant="quiet" onClick={() => onNavigate("audit")}>
                Open audit log
              </Button>
            </div>
            <ul className="activity-list">
              {data.activity.map((entry) => (
                <li className="activity" key={entry.id}>
                  <span className="activity__time">{entry.at}</span>
                  <span className="activity__text">
                    <strong>{entry.actor}</strong> {entry.action}{" "}
                    <span className="activity__subject">{entry.subject}</span>
                  </span>
                </li>
              ))}
            </ul>
          </section>
        </div>

        <aside className="mission-aside" aria-label="Event health">
          <section className="card" aria-labelledby="bot-health-title">
            <div className="card__header">
              <h2 id="bot-health-title">Bot health</h2>
              <StatusPill tone={botTone}>{data.bot.status}</StatusPill>
            </div>
            <div className="bot-card">
              <Icon name="bot" size={18} className="bot-card__mark" />
              <div>
                <p className="bot-card__name">{data.bot.label}</p>
                <p className="bot-card__detail">{data.bot.detail}</p>
                {data.bot.lastSeen ? (
                  <p className="bot-card__seen">
                    Last check-in {data.bot.lastSeen}
                  </p>
                ) : null}
              </div>
            </div>
            {data.bot.status === "Online" ? (
              <Button
                variant="secondary"
                icon="settings"
                onClick={() => onAction("Connection settings")}
              >
                Connection settings
              </Button>
            ) : null}
          </section>

          <section className="card" aria-labelledby="event-facts-title">
            <div className="card__header">
              <h2 id="event-facts-title">Event facts</h2>
            </div>
            <dl className="facts">
              {data.facts.map((fact) => (
                <div className="facts__row" key={fact.label}>
                  <dt>{fact.label}</dt>
                  <dd>{fact.value}</dd>
                </div>
              ))}
            </dl>
            <Button
              variant="quiet"
              icon="settings"
              onClick={() => onNavigate("settings")}
            >
              App settings
            </Button>
          </section>

        </aside>
      </div>

      {selectedDecision ? (
        <Dialog
          key={confirmMiss ? "missed-confirm" : "decision-review"}
          title={
            confirmMiss ? "Mark the turn missed?" : selectedDecision.title
          }
          initialFocusId="decision-cancel"
          onClose={() => {
            setSelectedDecision(null);
            setConfirmMiss(false);
          }}
          footer={
            <>
              <Button
                id="decision-cancel"
                variant="secondary"
                onClick={() => {
                  if (confirmMiss) {
                    setConfirmMiss(false);
                  } else {
                    setSelectedDecision(null);
                  }
                }}
              >
                Cancel
              </Button>
              {confirmMiss ? (
                <Button
                  variant="danger"
                  onClick={() => {
                    onDecision(selectedDecision, "missed");
                    setSelectedDecision(null);
                    setConfirmMiss(false);
                  }}
                >
                  Mark turn missed
                </Button>
              ) : selectedDecision.kind === "Extension" ? (
                <>
                  <Button
                    variant="danger"
                    className="dialog__danger-action"
                    onClick={() => {
                      onDecision(selectedDecision, "refuse");
                      setSelectedDecision(null);
                    }}
                  >
                    Refuse
                  </Button>
                  <Button
                    variant="primary"
                    onClick={() => {
                      onDecision(selectedDecision, "approve");
                      setSelectedDecision(null);
                    }}
                  >
                    Approve
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    variant="secondary"
                    onClick={() => {
                      onDecision(selectedDecision, "wait");
                      setSelectedDecision(null);
                    }}
                  >
                    Keep waiting
                  </Button>
                  <Button
                    variant="danger"
                    className="dialog__danger-action"
                    onClick={() => setConfirmMiss(true)}
                  >
                    Mark turn missed
                  </Button>
                  <Button
                    variant="primary"
                    disabled={(selectedDecision.extensionsUsed ?? 0) >= 2}
                    onClick={() => {
                      onDecision(selectedDecision, "extend");
                      setSelectedDecision(null);
                    }}
                  >
                    {(selectedDecision.extensionsUsed ?? 0) >= 2
                      ? "Extension limit reached"
                      : "Extend 6 hours"}
                  </Button>
                </>
              )}
            </>
          }
        >
          {confirmMiss ? (
            <>
              <p>
                The pair counts as seen. The track returns to the pool and the
                participant is notified. This cannot be undone.
              </p>
              <p className="dialog__meta">
                Cancel returns to the decision without a change.
              </p>
            </>
          ) : (
            <>
              <p>{selectedDecision.detail}</p>
              {selectedDecision.kind === "Extension" ? (
                <p className="dialog__meta">
                  Deadline {selectedDecision.deadline ?? "not set"}. Approving
                  moves it to {selectedDecision.newDeadline ?? "a later time"} and
                  uses extension {(selectedDecision.extensionsUsed ?? 0) + 1} of
                  2. Refusing keeps the deadline. The participant is notified
                  either way.
                </p>
              ) : (
                <p className="dialog__meta">
                  Nothing has moved. Deadline {selectedDecision.deadline ?? "passed"}. Extend adds
                  6 hours to {selectedDecision.newDeadline ?? "a later time"} and
                  uses extension {(selectedDecision.extensionsUsed ?? 0) + 1} of
                  2. Mark turn
                  missed counts the pair as seen, returns the track to the pool,
                  and cannot be undone. Keep waiting leaves this card open.
                </p>
              )}
            </>
          )}
        </Dialog>
      ) : null}

      {confirmLock ? (
        <Dialog
          title="Lock the roster?"
          initialFocusId="lock-cancel"
          onClose={() => setConfirmLock(false)}
          footer={
            <>
              <Button
                id="lock-cancel"
                variant="secondary"
                onClick={() => setConfirmLock(false)}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={() => {
                  setConfirmLock(false);
                  onAction("Lock roster");
                }}
              >
                Lock roster
              </Button>
            </>
          }
        >
          <p>
            Seed intake closes. Anyone without a seed becomes a fill-in. The app
            keeps every submitted seed and builds the draft schedule next. This
            cannot be undone without a host decision.
          </p>
        </Dialog>
      ) : null}

      {confirmReveal ? (
        <Dialog
          title="Reveal the event?"
          initialFocusId="reveal-cancel"
          onClose={() => setConfirmReveal(false)}
          footer={
            <>
              <Button
                id="reveal-cancel"
                variant="secondary"
                onClick={() => setConfirmReveal(false)}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirmReveal(false);
                  onAction("Reveal event");
                }}
              >
                Reveal event
              </Button>
            </>
          }
        >
          <p>
            Credits, participant names, and the assignment history become
            public in the reveal channel. This ends anonymity and cannot be
            undone. If the bot is offline, the message waits in the queue. Make
            sure every track is final before you continue.
          </p>
        </Dialog>
      ) : null}

      {moveDeadline ? (
        <Dialog
          title={`Move the Round ${event.round} deadline`}
          onClose={() => setMoveDeadline(null)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setMoveDeadline(null)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={async () => {
                  const deadlineAt = fromDateTimeInput(moveDeadline);
                  if (!Number.isFinite(deadlineAt)) {
                    return;
                  }
                  await onMoveRoundDeadline(
                    event.round,
                    deadlineAt,
                    moveReason || "host moved the round deadline",
                  );
                  setMoveDeadline(null);
                }}
              >
                Move deadline
              </Button>
            </>
          }
        >
          <p>
            The new deadline applies to offered, claimed, and overdue work in
            Round {event.round}. Overdue work returns to claimed. Later
            individual extensions are preserved. Submitted work stays final.
          </p>
          <Field
            label="New deadline"
            htmlFor="move-round-deadline"
            helpId="move-round-deadline-help"
            help="Use the host timezone. The bot sends the new time to every holder."
          >
            <input
              id="move-round-deadline"
              className="input"
              type="datetime-local"
              aria-describedby="move-round-deadline-help"
              value={moveDeadline}
              onChange={(event) => setMoveDeadline(event.target.value)}
            />
          </Field>
          <Field
            label="Reason"
            htmlFor="move-round-reason"
            helpId="move-round-reason-help"
            help="The reason appears in the audit log. It is not sent to participants."
          >
            <input
              id="move-round-reason"
              className="input"
              aria-describedby="move-round-reason-help"
              value={moveReason}
              onChange={(event) => setMoveReason(event.target.value)}
              placeholder="A server outage delayed three edits"
            />
          </Field>
        </Dialog>
      ) : null}
    </div>
  );
}

function FirstRun({ onAction }: { onAction: (label: string) => void }) {
  return (
    <div className="view">
      <PageHeader
        title="Set up Monsquaz Swap"
        description="Monsquaz Swap turns one seed per person into an anonymous album. Start with the bot, then open the event."
      />
      <div className="first-run">
        <ol className="setup-steps">
          <li>
            <span className="setup-steps__index">01</span>
            <div>
              <h2>Connect the bot</h2>
              <p>
                Add your own Discord bot token. It stays in secure system storage
                and never appears in this window.
              </p>
            </div>
          </li>
          <li>
            <span className="setup-steps__index">02</span>
            <div>
              <h2>Open the doors</h2>
              <p>
                Pick the server, the announcement channel, and the reveal
                channel. Invite the bot with the shown permissions.
              </p>
            </div>
          </li>
          <li>
            <span className="setup-steps__index">03</span>
            <div>
              <h2>Collect seeds</h2>
              <p>
                People join by sending one track. A signup without a seed
                becomes a fill-in when signups close.
              </p>
            </div>
          </li>
        </ol>
        <div className="first-run__action">
          <Button
            variant="primary"
            icon="arrow-right"
            onClick={() => onAction("Start setup")}
          >
            Start setup
          </Button>
          <p>
            You can run a dry run with fake participants before anything touches
            Discord.
          </p>
        </div>
      </div>
    </div>
  );
}

export function MissionControlSkeleton() {
  return (
    <div
      className="view"
      role="status"
      aria-busy="true"
      aria-label="Loading mission control"
    >
      <span className="visually-hidden">Loading mission control</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="120px" height={12} />
          <Skeleton width="380px" height={44} className="skeleton--title" />
          <Skeleton width="260px" height={16} />
          <Skeleton width="420px" height={14} />
        </div>
      </div>
      <div className="next-action next-action--skeleton">
        <div className="next-action__copy">
          <Skeleton width="90px" height={12} />
          <Skeleton width="340px" height={28} />
          <Skeleton width="100%" height={16} />
          <Skeleton width="70%" height={16} />
        </div>
        <div className="next-action__actions">
          <Skeleton width="150px" height={40} />
          <Skeleton width="120px" height={20} />
        </div>
      </div>
      <div className="mission-grid">
        <div className="mission-main">
          <section className="card">
            <div className="card__header">
              <Skeleton width="160px" height={22} />
              <Skeleton width="80px" height={14} />
            </div>
            <div className="skeleton-stack">
              <Skeleton width="100%" height={7} />
            </div>
            <div className="skeleton-timeline">
              {Array.from({ length: 6 }, (_, index) => (
                <div key={index}>
                  <Skeleton width="20px" height={20} />
                  <Skeleton width="80%" height={12} />
                  <Skeleton width="60%" height={10} />
                </div>
              ))}
            </div>
          </section>
          <SkeletonCard lines={4} />
          <SkeletonCard lines={5} />
        </div>
        <aside className="mission-aside">
          <SkeletonCard lines={3} />
          <SkeletonCard lines={6} />
        </aside>
      </div>
    </div>
  );
}

function SkeletonCard({ lines }: { lines: number }) {
  return (
    <section className="card">
      <Skeleton width="45%" height={22} />
      <div className="skeleton-stack">
        {Array.from({ length: lines }, (_, index) => (
          <Skeleton
            key={index}
            width={index % 3 === 0 ? "92%" : index % 3 === 1 ? "78%" : "64%"}
            height={14}
          />
        ))}
      </div>
    </section>
  );
}

function defaultSeedDeadline() {
  return Math.floor(Date.now() / 1000) + 7 * 24 * 60 * 60;
}

function toDateTimeInput(epochSeconds: number) {
  const date = new Date(epochSeconds * 1000);
  const offset = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

function fromDateTimeInput(value: string) {
  return Math.floor(new Date(value).getTime() / 1000);
}

function formatDeadline(epochSeconds: number, timezone?: string) {
  return new Intl.DateTimeFormat("en-GB", {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: timezone,
  }).format(new Date(epochSeconds * 1000));
}

function SeedRoundCard({
  event,
  lateSeeds,
  timezone,
  onUploadPack,
  onOpenSeedRound,
  onCloseSeedRound,
  onExtendSeedDeadline,
  onDecideLateSeed,
}: {
  event: NonNullable<MissionControl["event"]>;
  lateSeeds: MissionControl["lateSeeds"];
  timezone?: string;
  onUploadPack: (path: string) => Promise<PackInfo>;
  onOpenSeedRound: (deadlineAt: number, channelId: string | null) => Promise<void>;
  onCloseSeedRound: () => Promise<void>;
  onExtendSeedDeadline: (deadlineAt: number) => Promise<void>;
  onDecideLateSeed: (id: string, accept: boolean) => Promise<void>;
}) {
  const [pack, setPack] = useState<PackInfo | null>(null);
  const [deadline, setDeadline] = useState(() =>
    toDateTimeInput(event.seedDeadlineAt ?? defaultSeedDeadline()),
  );
  const [busy, setBusy] = useState(false);
  const [extending, setExtending] = useState(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const [error, setError] = useState<string | null>(() =>
    new URLSearchParams(window.location.search).get("seedError") === "1"
      ? "The file is larger than 20 MiB."
      : null,
  );
  const [deadlineError, setDeadlineError] = useState<string | null>(null);

  useEffect(() => {
    if (extending) {
      document.getElementById("seed-deadline-move")?.focus();
    }
  }, [extending]);

  function focusTitle() {
    document.getElementById("seed-round-title")?.focus();
  }
  const isTauri = "__TAURI_INTERNALS__" in window;

  async function choosePack() {
    let path = "demo-pack.zip";
    if (isTauri) {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Host pack",
            extensions: ["zip", "7z", "rar", "it", "xm", "mod", "s3m"],
          },
        ],
      });
      if (!picked || Array.isArray(picked)) {
        return;
      }
      path = picked;
    }
    setBusy(true);
    setError(null);
    try {
      setPack(await onUploadPack(path));
    } catch (uploadError) {
      setError(
        uploadError instanceof Error
          ? uploadError.message
          : "The host pack could not be uploaded.",
      );
    } finally {
      setBusy(false);
    }
  }

  async function openRound() {
    const deadlineAt = fromDateTimeInput(deadline);
    if (!Number.isFinite(deadlineAt)) {
      setDeadlineError("Choose a date and a time for the seed deadline.");
      return;
    }
    setDeadlineError(null);
    setBusy(true);
    try {
      await onOpenSeedRound(deadlineAt, null);
      focusTitle();
    } finally {
      setBusy(false);
    }
  }

  async function closeRound() {
    setBusy(true);
    try {
      await onCloseSeedRound();
      setConfirmClose(false);
      focusTitle();
    } finally {
      setBusy(false);
    }
  }

  async function extend() {
    const deadlineAt = fromDateTimeInput(deadline);
    if (!Number.isFinite(deadlineAt)) {
      setDeadlineError("Choose a date and a time for the seed deadline.");
      return;
    }
    setDeadlineError(null);
    setBusy(true);
    try {
      await onExtendSeedDeadline(deadlineAt);
      setExtending(false);
      document.getElementById("extend-seed-deadline")?.focus();
    } finally {
      setBusy(false);
    }
  }

  const ready = pack !== null || event.packId !== null;

  return (
    <section className="card seed-round-card" aria-labelledby="seed-round-title">
      <div className="card__header">
        <div>
          <h2 id="seed-round-title" tabIndex={-1}>
            Round 1 · Seed round
          </h2>
          <p className="card__subtitle">
            Every participant makes one track from the host pack. Round 1 is the
            seed round and counts in the event length: {event.participants || "N"}{" "}
            participants make {event.rounds || "N"} rounds. Rounds 2 to{" "}
            {event.rounds || "N"} are exchanges.
          </p>
        </div>
        <StatusPill tone={event.seedRoundOpen ? "ok" : "info"}>
          {event.seedRoundOpen ? "Open" : "Not open"}
        </StatusPill>
      </div>

      {error ? (
        <div className="banner banner--danger" role="alert">
          <Icon name="alert" size={20} />
          <div>
            <strong>The host pack could not be uploaded.</strong> {error} Try
            again, or choose a different file.
          </div>
        </div>
      ) : null}

      {!event.seedRoundOpen ? (
        <>
          <div className="seed-round-row">
            <div>
              <strong>Host pack</strong>
              <p className="field-help">
                Samples and an empty module. Allowed files: zip, 7z, rar, it,
                xm, mod, s3m. Bring-your-own is a separate option in setup.
              </p>
              <p className="card__meta">
                {pack
                  ? `${pack.filename} · ${(pack.size / (1024 * 1024)).toFixed(1)} MiB`
                  : event.packId
                    ? `Pack ${event.packId} is ready`
                    : "No pack uploaded yet"}
              </p>
            </div>
            <Button
              variant="secondary"
              icon="file"
              onClick={choosePack}
              disabled={busy}
            >
              {pack || event.packId ? "Replace host pack…" : "Upload host pack…"}
            </Button>
          </div>

          <div className="seed-round-row seed-round-row--field">
            <Field
              label={`Seed deadline${timezone ? ` (${timezone})` : ""}`}
              htmlFor="seed-deadline"
              helpId="seed-deadline-help"
              help={`Round 1 closes at ${formatDeadline(
                fromDateTimeInput(deadline),
                timezone,
              )}. The default is 7 days after you open Round 1. A late seed does not count until you accept it.`}
            >
              <input
                id="seed-deadline"
                className="input"
                type="datetime-local"
                aria-describedby="seed-deadline-help"
                aria-invalid={deadlineError ? true : undefined}
                value={deadline}
                onChange={(event) => setDeadline(event.target.value)}
              />
            </Field>
            <Button
              variant="primary"
              icon="play"
              onClick={openRound}
              disabled={busy || !ready}
              describedBy={!ready ? "seed-open-help" : undefined}
            >
              Open Round 1
            </Button>
          </div>
          {deadlineError ? (
            <p className="field-help field-help--danger" role="alert">
              {deadlineError}
            </p>
          ) : null}
          {!ready ? (
            <p className="field-help" id="seed-open-help">
              Upload the host pack to open Round 1. The pack is stored locally
              and sent by DM. It never travels through the app window.
            </p>
          ) : null}
        </>
      ) : (
        <>
          {lateSeeds.length > 0 ? (
            <p className="field-help field-help--danger" id="late-seed-help">
              Accept or decline {lateSeeds.length} late{" "}
              {lateSeeds.length === 1 ? "seed" : "seeds"} before you close Round
              1.
            </p>
          ) : null}
          {extending ? (
            <div className="seed-round-row">
              <Field
                label="New seed deadline"
                htmlFor="seed-deadline-move"
                helpId="seed-deadline-move-help"
                help="The new deadline applies to late seeds and the roster close."
              >
                <input
                  id="seed-deadline-move"
                  className="input"
                  type="datetime-local"
                  aria-describedby="seed-deadline-move-help"
                  value={deadline}
                  onChange={(event) => setDeadline(event.target.value)}
                />
              </Field>
              <div className="header-actions">
                <Button variant="secondary" onClick={() => setExtending(false)}>
                  Cancel
                </Button>
                <Button variant="primary" onClick={extend} disabled={busy}>
                  Save deadline
                </Button>
              </div>
            </div>
          ) : (
            <div className="header-actions seed-round-actions">
              <Button
                variant="primary"
                icon="lock"
                onClick={() => setConfirmClose(true)}
                disabled={
                  busy || event.seeded < 2 || lateSeeds.length > 0
                }
                describedBy={
                  lateSeeds.length > 0
                    ? "late-seed-help"
                    : event.seeded < 2
                      ? "seed-minimum-help"
                      : undefined
                }
              >
                Close seed round and lock roster
              </Button>
              <Button
                id="extend-seed-deadline"
                variant="secondary"
                icon="clock"
                onClick={() => setExtending(true)}
                disabled={busy}
              >
                Extend deadline
              </Button>
            </div>
          )}
          <p className="field-help">
            Closing stops seed intake and locks the roster. New participants
            cannot join. Rounds 2 to {event.rounds} start when you continue.
          </p>
          {event.seeded < 2 ? (
            <p className="field-help" id="seed-minimum-help">
              At least two seeds are needed before the roster can lock.
            </p>
          ) : null}
          <dl className="review-grid review-grid--three">
            <div>
              <dt>Seed deadline{timezone ? ` (${timezone})` : ""}</dt>
              <dd>
                {event.seedDeadlineAt
                  ? formatDeadline(event.seedDeadlineAt, timezone)
                  : formatDeadline(fromDateTimeInput(deadline), timezone)}
              </dd>
            </div>
            <div>
              <dt>Seeds in</dt>
              <dd aria-live="polite">
                {event.seeded} of {event.participants} participants. Minimum to
                lock the roster: 2.
              </dd>
            </div>
            <div>
              <dt>Pack</dt>
              <dd>
                {event.packId ? `Uploaded · ${event.packId}` : "Bring your own"}
              </dd>
            </div>
          </dl>

          {lateSeeds.length > 0 ? (
            <div className="warnings" role="status" aria-live="polite">
              {lateSeeds.map((seed) => (
                <div className="warning warning--warn" key={seed.id}>
                  <Icon name="clock" size={18} />
                  <div>
                    <strong>{seed.displayName} sent a late seed</strong>
                    <p>
                      Track {seed.trackCode}. Accept it before you close Round 1.
                    </p>
                    <div className="header-actions">
                      <Button
                        variant="secondary"
                        ariaLabel={`Accept ${seed.displayName}'s late seed`}
                        onClick={() => onDecideLateSeed(seed.id, true)}
                      >
                        Accept seed
                      </Button>
                      <Button
                        variant="quiet"
                        ariaLabel={`Decline ${seed.displayName}'s late seed`}
                        onClick={() => onDecideLateSeed(seed.id, false)}
                      >
                        Decline
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          ) : null}
        </>
      )}

      {confirmClose ? (
        <Dialog
          title="Close the seed round?"
          onClose={() => setConfirmClose(false)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setConfirmClose(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                icon="lock"
                onClick={closeRound}
                disabled={busy}
              >
                Close and lock roster
              </Button>
            </>
          }
        >
          <p>
            Seed intake stops and the roster locks with {event.seeded} seeded
            participants. New participants cannot join after this. The host can
            reopen the seed round before the schedule starts.
          </p>
        </Dialog>
      ) : null}
    </section>
  );
}
