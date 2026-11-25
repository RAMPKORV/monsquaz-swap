import { useEffect, useMemo, useState } from "react";
import type { PlanData } from "../types";
import { Button, Dialog, EmptyState, PageHeader, Skeleton, StatusPill } from "../components";
import { Icon } from "../icons";


export function PlannerView({
  data,
  onAction,
  onGeneratePlan,
  onStartEvent,
  onRedactionChange,
}: {
  data: PlanData;
  onAction: (label: string) => void;
  onGeneratePlan: () => void;
  onStartEvent: () => void;
  onRedactionChange: (active: boolean) => void;
}) {
  const [viewAsParticipant, setViewAsParticipant] = useState(
    () => new URLSearchParams(window.location.search).get("participant") === "1",
  );
  const [confirmLock, setConfirmLock] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState(false);
  useEffect(() => {
    onRedactionChange(viewAsParticipant);
    return () => onRedactionChange(false);
  }, [onRedactionChange, viewAsParticipant]);
  const viewer = useMemo(() => {
    const cells = data.matrix.flatMap((track) => track.cells);
    return (
      cells.find((cell) => cell.participantId === "user-04") ??
      cells.find((cell) => cell.participantId.length > 0) ??
      null
    );
  }, [data.matrix]);
  const eligible = useMemo(
    () =>
      viewAsParticipant
        ? data.graph.edges.filter(
            (edge) => edge.participantName === viewer?.participantName,
          )
        : data.graph.edges,
    [data.graph.edges, viewAsParticipant, viewer],
  );

  if (data.status === "No event") {
    return (
      <div className="view">
        <PageHeader
          title="Schedule planner"
          description="The matrix and graph show every assignment. No owner, title, or comment is exposed before the reveal."
        />
        <EmptyState
          icon="shuffle"
          title="No event yet"
          detail="Create an event and collect seeds first. The planner can then build the matrix and the round graph."
          action={
            <Button
              variant="secondary"
              onClick={() => onAction("Open event setup")}
            >
              Open event setup
            </Button>
          }
        />
      </div>
    );
  }

  const noPlan = data.status === "No plan";
  const infeasible = data.status === "Infeasible";
  const matrixRounds = data.matrix.reduce(
    (most, track) => Math.max(most, track.cells.length),
    0,
  );
  const visibleWarnings = infeasible ? data.warnings.slice(1) : data.warnings;

  return (
    <div className="view">
      <PageHeader
        title={viewAsParticipant ? "Redaction preview" : "Schedule planner"}
        description={
          viewAsParticipant
            ? "Only the sample participant's assignments are visible. This is not the participant app."
            : `${data.event?.name ?? "Event"}. The matrix and graph show every assignment.`
        }
        actions={
          <div className="header-actions">
            {viewAsParticipant ? null : (
              <StatusPill
                tone={
                  infeasible
                    ? "danger"
                    : noPlan
                      ? "muted"
                      : data.committed
                        ? "ok"
                        : "info"
                }
              >
                {data.status}
              </StatusPill>
            )}
            <Button
              variant="secondary"
              icon={viewAsParticipant ? "eye-off" : "users"}
              onClick={() => setViewAsParticipant(!viewAsParticipant)}
            >
              {viewAsParticipant
                ? "Exit redaction preview"
                : "Preview participant redaction"}
            </Button>
          </div>
        }
      />

      {viewAsParticipant ? (
        <div className="banner banner--info" role="status">
          <Icon name="eye-off" size={20} />
          <div>
            <strong>Redaction preview, not the participant app.</strong> These
            are the planner fields participants must not see. A participant sees
            only their own offers, the tracks they have seen, and the dates.
            Sample participant: {viewer ? shortName(viewer.participantName) : "none"}.
          </div>
        </div>
      ) : null}

      <div
        className={`plan-summary${
          viewAsParticipant ? " plan-summary--participant" : ""
        }`}
      >
        <SummaryFact label="Rounds" value={String(data.rounds)} />
        <SummaryFact label="Tracks" value={String(data.tracks)} />
        <SummaryFact label="People" value={String(data.participants)} />
        {!viewAsParticipant ? (
          <SummaryFact label="Groups" value={String(data.groups.length)} />
        ) : null}
      </div>

      {!noPlan && !infeasible && !viewAsParticipant && !data.committed ? (
        <section className="plan-actionbar" aria-label="Plan actions">
          <div className="plan-actionbar__copy">
            <strong>Draft schedule</strong>
            <span>
              Locking posts a fingerprint of this schedule in the announcement
              channel. People can use it to check that the schedule did not
              change. A new version replaces this one and is visible to
              participants.
            </span>
          </div>
          <div className="plan-actions">
            <Button
              variant="secondary"
              icon="refresh"
              onClick={() => setConfirmReplace(true)}
            >
              Replace draft…
            </Button>
            <Button
              variant="primary"
              icon="lock"
              onClick={() => setConfirmLock(true)}
            >
              Lock schedule
            </Button>
          </div>
        </section>
      ) : null}

      {noPlan ? (
        <section className="planner-empty">
          <EmptyState
            icon="shuffle"
            title="No plan committed yet"
            detail="Generate a draft, inspect the matrix and graph, then lock the schedule. Locking posts a public fingerprint of the schedule. No music file is shared."
            action={
              <Button
                variant="primary"
                icon="shuffle"
                onClick={onGeneratePlan}
              >
                Generate plan
              </Button>
            }
          />
        </section>
      ) : null}

      {infeasible ? (
        <section className="infeasible-panel" role="alert">
          <span className="infeasible-panel__icon">
            <Icon name="alert" size={24} />
          </span>
          <div>
            <h2>{data.warnings[0]?.title ?? "This round cannot be filled"}</h2>
            <p>
              {data.warnings[0]?.detail ??
                "The planner cannot fill this round with the current pool."}
            </p>
          </div>
          <div className="infeasible-panel__actions">
            <div className="infeasible-panel__action-copy">
              <Button
                variant="primary"
                icon="shuffle"
                onClick={onGeneratePlan}
              >
                Use anyone eligible
              </Button>
              <span className="field-help">
                Draws from the global pool for this round. The no-repeat rule
                still applies.
              </span>
            </div>
          </div>
        </section>
      ) : null}

      {!noPlan ? (
        <div
          className={`planner-grid${
            viewAsParticipant ? " planner-grid--participant" : ""
          }`}
        >
          <div className="planner-main">
            <section className="card matrix-card" aria-labelledby="matrix-title">
              <div className="card__header">
                <div>
                  <h2 id="matrix-title">Assignment matrix</h2>
                  <p className="card__subtitle">
                    {viewAsParticipant
                      ? "Only the sample participant's assignments are visible."
                      : "Tracks by round. Each cell is the person who holds the track."}
                  </p>
                </div>
                {data.matrix.length > 16 ? (
                  <span className="card__meta">
                    Scroll for all {data.matrix.length} tracks
                  </span>
                ) : null}
              </div>
              <div
                className={`matrix-scroll ${
                  data.matrix.length > 16 ? "matrix-scroll--bounded" : ""
                }`}
                tabIndex={0}
                aria-label="Assignment matrix, scroll both axes"
              >
                <table className="matrix">
                  <caption className="visually-hidden">
                    Track by round assignment matrix
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">Track</th>
                      {Array.from({ length: matrixRounds }, (_, index) => (
                        <th scope="col" key={index}>R{index + 1}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {data.matrix.map((track) => (
                      <tr key={track.trackCode}>
                        <th scope="row">
                          <code>{track.trackCode}</code>
                          {viewAsParticipant ? null : (
                            <small title={`Group ${track.ownerGroup}`}>
                              {track.ownerGroup}
                            </small>
                          )}
                        </th>
                        {track.cells.map((cell) => {
                          const mine = cell.participantId === viewer?.participantId;
                          return (
                            <td key={cell.round}>
                              {viewAsParticipant ? (
                                mine ? <span className="matrix-you">You</span> : <span className="matrix-blank">—</span>
                              ) : (
                                <span title={cell.participantName}>
                                  {shortName(cell.participantName)}
                                </span>
                              )}
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>

            <section className="card graph-card" aria-labelledby="graph-title">
              <div className="card__header">
                <div>
                  <h2 id="graph-title">Round {data.graph.round + 1} graph</h2>
                  <p className="card__subtitle">
                    {viewAsParticipant
                      ? "The sample participant's edge only."
                      : "Each edge is one assignment. Tracks stay anonymous."}
                  </p>
                </div>
              </div>
              <PlanGraph
                data={data}
                edges={eligible}
                participantView={viewAsParticipant}
              />
            </section>

            {visibleWarnings.length > 0 ? (
              <section className="card" aria-labelledby="warnings-title">
                <div className="card__header">
                  <h2 id="warnings-title">Planner warnings</h2>
                  <StatusPill tone={infeasible ? "danger" : "warn"}>
                    {visibleWarnings.length} to review
                  </StatusPill>
                </div>
                <div className="warnings">
                  {visibleWarnings.map((warning) => (
                    <div className={`warning warning--${warning.severity}`} key={warning.id}>
                      <Icon
                        name={warning.severity === "danger" ? "alert" : "check"}
                        size={18}
                      />
                      <div>
                        <strong>{warning.title}</strong>
                        <p>{warning.detail}</p>
                      </div>
                    </div>
                  ))}
                </div>
              </section>
            ) : null}
          </div>

          <aside className="planner-aside" aria-label="Planner details">
            {!viewAsParticipant ? (
              <section className="card" aria-labelledby="fun-title">
                <div className="card__header">
                  <div>
                    <h2 id="fun-title">Fun score</h2>
                    <p className="card__subtitle">Host-only planning aid.</p>
                  </div>
                  {data.funScore === null ? (
                    <span className="fun-score fun-score--empty">Not scored</span>
                  ) : (
                    <span className="fun-score">{data.funScore}</span>
                  )}
                </div>
                <dl className="facts">
                  {data.funGuardrails.map((guardrail) => (
                    <div className="facts__row" key={guardrail.label}>
                      <dt>{guardrail.label}</dt>
                      <dd>{guardrail.value}</dd>
                    </div>
                  ))}
                </dl>
                <p className="field-help">
                  Not a participant grade. It estimates reach, variety, fairness,
                  and freshness.
                </p>
              </section>
            ) : null}

            {!viewAsParticipant ? (
              <section className="card" aria-labelledby="groups-title">
                <div className="card__header">
                  <h2 id="groups-title">Groups</h2>
                  <span className="card__meta">{data.groups.length} groups</span>
                </div>
                <ul className="group-list">
                  {data.groups.map((group) => (
                    <li key={group.name}>
                      <span className="group-list__name">Group {group.name}</span>
                      <span className="group-list__meta">
                        {group.size} people, {group.tracks} tracks
                      </span>
                      <span
                        className="group-list__people"
                        title={group.participants.join(", ")}
                      >
                        {group.participants.join(", ")}
                      </span>
                    </li>
                  ))}
                </ul>
                <p className="field-help">
                  Participants never see group names or membership.
                </p>
              </section>
            ) : null}

          </aside>
        </div>
      ) : null}

      {confirmReplace ? (
        <Dialog
          title="Replace this draft?"
          onClose={() => setConfirmReplace(false)}
          footer={
            <>
              <Button
                variant="secondary"
                onClick={() => setConfirmReplace(false)}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                icon="refresh"
                onClick={() => {
                  setConfirmReplace(false);
                  onGeneratePlan();
                }}
              >
                Replace draft
              </Button>
            </>
          }
        >
          <p>
            The current draft is discarded. The new draft uses the same roster
            and rules. Review the matrix before you lock it.
          </p>
        </Dialog>
      ) : null}

      {confirmLock ? (
        <Dialog
          title="Lock this schedule?"
          onClose={() => setConfirmLock(false)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setConfirmLock(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                icon="lock"
                onClick={() => {
                  setConfirmLock(false);
                  onStartEvent();
                }}
              >
                Lock schedule
              </Button>
            </>
          }
        >
          <p>
            The bot posts a fingerprint, a short code, of this schedule in the
            announcement channel. Every offer follows it. If you change an
            assignment later, you must publish a new version, and people can see
            that it changed.
          </p>
          <p className="field-help">
            No music file is posted. Locking does not reveal an owner, a track
            title, or a comment.
          </p>
        </Dialog>
      ) : null}
    </div>
  );
}

function SummaryFact({
  label,
  value,
  highlight = false,
}: {
  label: string;
  value: string;
  highlight?: boolean;
}) {
  return (
    <div className={`summary-fact ${highlight ? "summary-fact--highlight" : ""}`}>
      <span className="summary-fact__value">{value}</span>
      <span className="summary-fact__label">{label}</span>
    </div>
  );
}

function PlanGraph({
  data,
  edges,
  participantView,
}: {
  data: PlanData;
  edges: PlanData["graph"]["edges"];
  participantView: boolean;
}) {
  if (edges.length === 0) {
    return (
      <p className="empty-graph">
        {data.status === "Infeasible"
          ? "No graph is drawn for a refused round."
          : "No assignments to draw."}
      </p>
    );
  }
  const height = Math.max(180, edges.length * 34 + 54);
  const tracks = unique(edges.map((edge) => edge.trackCode));
  const people = unique(edges.map((edge) => edge.participantName)).sort((left, right) =>
    left.localeCompare(right),
  );
  const trackY = new Map(tracks.map((track, index) => [track, 28 + index * 30]));
  const personY = new Map(people.map((person, index) => [person, 28 + index * 30]));

  return (
    <div className="graph-wrap">
      <svg
        viewBox={`0 0 720 ${height}`}
        role="img"
        aria-labelledby="graph-title graph-desc"
        className="plan-graph"
        preserveAspectRatio="xMidYMid meet"
      >
        <title id="graph-title">Round {data.graph.round + 1} assignment graph</title>
        <desc id="graph-desc">
          Tracks on the left connect to the people who hold them in this round.
          The graph does not show owners or group membership. The assignment
          matrix above lists every track and person pair.
        </desc>
        {edges.map((edge) => (
          <g key={`${edge.trackCode}-${edge.participantName}`}>
            <line
              x1="154"
              y1={trackY.get(edge.trackCode) ?? 0}
              x2="516"
              y2={personY.get(edge.participantName) ?? 0}
              className={`graph-edge ${participantView ? "graph-edge--you" : ""}`}
            />
            <circle
              cx="154"
              cy={trackY.get(edge.trackCode) ?? 0}
              r="2.5"
              className="graph-node"
            />
            <circle
              cx="516"
              cy={personY.get(edge.participantName) ?? 0}
              r="2.5"
              className="graph-node"
            />
          </g>
        ))}
        {tracks.map((track) => (
          <text key={track} x="144" y={(trackY.get(track) ?? 0) + 4} textAnchor="end" className="graph-label">
            {track}
          </text>
        ))}
        {people.map((person) => (
          <text key={person} x="526" y={(personY.get(person) ?? 0) + 4} className="graph-label">
            {shortName(person)}
          </text>
        ))}
        <text x="24" y="16" className="graph-caption">
          Track
        </text>
        <text x="620" y="16" className="graph-caption">
          Person, round {data.graph.round + 1}
        </text>
        <line x1="154" y1="26" x2="154" y2={height - 18} className="graph-rail" />
        <line x1="516" y1="26" x2="516" y2={height - 18} className="graph-rail" />
      </svg>
      <p className="graph-legend">
        <span className="graph-legend__line" /> The graph does not name a track
        owner.
      </p>
    </div>
  );
}

function unique(values: string[]): string[] {
  return Array.from(new Set(values));
}

function shortName(name: string): string {
  return name.split(/\s+/)[0] ?? name;
}

export function PlannerSkeleton() {
  return (
    <div className="view" role="status" aria-busy="true" aria-label="Loading schedule planner">
      <span className="visually-hidden">Loading schedule planner</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="130px" height={12} />
          <Skeleton width="460px" height={44} className="skeleton--title" />
          <Skeleton width="min(100%, 620px)" height={16} />
        </div>
      </div>
      <div className="plan-summary">
        {Array.from({ length: 5 }, (_, index) => (
          <Skeleton key={index} width="100%" height={72} />
        ))}
      </div>
      <div className="planner-grid">
        <div className="planner-main">
          <Skeleton width="100%" height={360} />
          <Skeleton width="100%" height={300} />
        </div>
        <aside className="planner-aside">
          <Skeleton width="100%" height={240} />
          <Skeleton width="100%" height={220} />
        </aside>
      </div>
    </div>
  );
}
