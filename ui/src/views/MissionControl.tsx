import { useState } from "react";
import type { CSSProperties } from "react";
import type { MissionControl } from "../types";
import {
  Button,
  Dialog,
  EmptyState,
  Meter,
  PageHeader,
  Skeleton,
  StatusPill,
} from "../components";
import { Icon } from "../icons";

function stageTone(stage: string) {
  if (stage === "Active") return "ok" as const;
  if (stage === "Paused") return "warn" as const;
  if (stage === "Reveal") return "info" as const;
  return "muted" as const;
}

export function MissionControlView({
  data,
  onAction,
  onNavigate,
}: {
  data: MissionControl;
  onAction: (label: string) => void;
  onNavigate: (view: "audit" | "settings") => void;
}) {
  const [confirmPause, setConfirmPause] = useState(false);
  if (data.firstRun || !data.event) {
    return <FirstRun onAction={onAction} />;
  }

  const event = data.event;
  const botTone =
    data.bot.status === "Online"
      ? "ok"
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
            {event.stage === "Active" ? (
              <Button
                variant="quiet"
                icon="pause"
                onClick={() => setConfirmPause(true)}
              >
                Pause event…
              </Button>
            ) : null}
          </div>
        }
      />

      {data.bot.status === "Offline" ? (
        <div className="banner banner--danger" role="status">
          <Icon name="alert" size={20} />
          <div>
            <strong>Bot offline.</strong> Offers and direct messages are paused.
            Claims and submissions wait safely in the local queue.
          </div>
        </div>
      ) : data.bot.status === "Reconnecting" ? (
        <div className="banner banner--warn" role="status">
          <Icon name="clock" size={20} />
          <div>
            <strong>Bot reconnecting.</strong> {data.bot.detail} The queue keeps
            every unsent effect.
          </div>
        </div>
      ) : null}

      <p className="privacy-line">
        <Icon name="shield" size={16} />
        Participants are anonymous to each other until the reveal. You can see
        the mapping.
      </p>

      {data.nextAction ? (
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
              onClick={() => onAction(data.nextAction?.actionLabel ?? "")}
            >
              {data.nextAction.actionLabel}
            </Button>
            {data.nextAction.secondaryLabel &&
            !/^pause/i.test(data.nextAction.secondaryLabel) ? (
              <Button
                variant="quiet"
                icon="pause"
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
                    ? "The schedule has not started."
                    : `${event.rounds - event.round} rounds remain after this one.`}
                </p>
              </div>
              <span className="card__meta">
                {event.round === 0
                  ? "Not started"
                  : `Round ${event.round} of ${event.rounds}`}
              </span>
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
                <h2 id="decisions-title">Decisions waiting</h2>
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
                detail="The round is healthy. Reminders and deadlines run on their own."
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
                      <Button
                        variant="secondary"
                        ariaLabel={`${decision.actionLabel}: ${decision.title}`}
                        onClick={() => onAction(decision.title)}
                      >
                        {decision.actionLabel}
                      </Button>
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
                icon="refresh"
                onClick={() => onAction("Test connection")}
              >
                Test connection
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

      {confirmPause ? (
        <Dialog
          title="Pause the event?"
          onClose={() => setConfirmPause(false)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setConfirmPause(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                icon="pause"
                onClick={() => {
                  setConfirmPause(false);
                  onAction("Pause event");
                }}
              >
                Pause event
              </Button>
            </>
          }
        >
          <p>
            Offers stop and reminders pause. Every deadline moves later by the
            pause length. People keep the tracks they hold.
          </p>
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
