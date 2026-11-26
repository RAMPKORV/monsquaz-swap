import {
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import type { PlanData, PlanEditCandidate } from "../types";
import { Button, Dialog, EmptyState, PageHeader, Skeleton, StatusPill } from "../components";
import { Icon } from "../icons";
import { plural } from "../i18n";


export function PlannerView({
  data,
  onAction,
  onGeneratePlan,
  onEditAssignment,
  onLoadCandidates,
  onRepairGroups,
  onStartEvent,
  onRedactionChange,
}: {
  data: PlanData;
  onAction: (label: string) => void;
  onGeneratePlan: () => void;
  onEditAssignment: (
    round: number,
    trackCode: string,
    participantId: string,
    expectedVersion: number,
  ) => Promise<void>;
  onLoadCandidates: (
    round: number,
    trackCode: string,
  ) => Promise<PlanEditCandidate[]>;
  onRepairGroups: () => void;
  onStartEvent: () => void;
  onRedactionChange: (active: boolean) => void;
}) {
  const [viewAsParticipant, setViewAsParticipant] = useState(
    () => new URLSearchParams(window.location.search).get("participant") === "1",
  );
  const [confirmLock, setConfirmLock] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState(false);
  const [confirmRebuild, setConfirmRebuild] = useState(false);
  const [editCell, setEditCell] = useState<{
    round: number;
    trackCode: string;
  } | null>(() => {
    if (new URLSearchParams(window.location.search).get("dialog") !== "edit") {
      return null;
    }
    const cell = data.matrix[0]?.cells[0];
    return cell ? { round: cell.round, trackCode: cell.trackCode } : null;
  });
  const [editCandidate, setEditCandidate] = useState("");
  const [editError, setEditError] = useState("");
  const [editBusy, setEditBusy] = useState(false);
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
  const canEdit = data.status === "Draft" && !viewAsParticipant;
  const editTrack = editCell
    ? data.matrix.find((track) => track.trackCode === editCell.trackCode)
    : undefined;
  const editTarget = editCell
    ? editTrack?.cells.find((cell) => cell.round === editCell.round)
    : undefined;
  const [editCandidates, setEditCandidates] = useState<PlanEditCandidate[]>([]);
  const [editCandidatesLoading, setEditCandidatesLoading] = useState(false);
  const [selectedGroup, setSelectedGroup] = useState<string | null>(
    () => new URLSearchParams(window.location.search).get("group"),
  );

  useEffect(() => {
    if (!editCell) {
      setEditCandidates([]);
      setEditCandidate("");
      setEditError("");
      return;
    }
    let cancelled = false;
    setEditCandidate("");
    setEditError("");
    setEditCandidates([]);
    setEditCandidatesLoading(true);
    onLoadCandidates(editCell.round, editCell.trackCode)
      .then((candidates) => {
        if (!cancelled) {
          setEditCandidates(candidates);
          if (
            candidates.length > 0 &&
            new URLSearchParams(window.location.search).get("candidate") ===
              "first"
          ) {
            setEditCandidate(candidates[0].participantId);
          }
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setEditError(
            error instanceof Error ? error.message : String(error),
          );
        }
      })
      .finally(() => {
        if (!cancelled) {
          setEditCandidatesLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [editCell, onLoadCandidates]);

  const editCandidateInfo = editCandidates.find(
    (candidate) => candidate.participantId === editCandidate,
  );

  useEffect(() => {
    if (viewAsParticipant) {
      setSelectedGroup(null);
    }
  }, [viewAsParticipant]);

  useEffect(() => {
    if (
      new URLSearchParams(window.location.search).get("focus") === "cell"
    ) {
      document
        .querySelector<HTMLButtonElement>('[data-matrix-cell="0-0"]')
        ?.focus();
    }
  }, []);

  useEffect(() => {
    if (editCandidates.length > 0) {
      document.getElementById("edit-holder")?.focus();
    }
  }, [editCandidates]);

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
  const selectedGroupCount = selectedGroup
    ? data.matrix.filter((track) => track.ownerGroup === selectedGroup).length
    : 0;
  const fallbackPhases = data.phases
    .filter((phase) => phase.globalPool)
    .map((phase) => phase.index + 1);
  const fallbackLabel =
    fallbackPhases.length === 1
      ? `phase ${fallbackPhases[0]}`
      : `phases ${fallbackPhases.slice(0, -1).join(", ")} and ${fallbackPhases[fallbackPhases.length - 1]}`;
  const phaseRanges = data.phases
    .map(
      (phase) =>
        `R${phase.startRound + 2}–${phase.startRound + phase.rounds + 1}`,
    )
    .join(" · ");

  function moveMatrixFocus(
    event: ReactKeyboardEvent<HTMLButtonElement>,
    trackIndex: number,
    roundIndex: number,
  ) {
    let nextTrack = trackIndex;
    let nextRound = roundIndex;
    if (event.key === "ArrowUp") {
      nextTrack = Math.max(0, trackIndex - 1);
    } else if (event.key === "ArrowDown") {
      nextTrack = Math.min(data.matrix.length - 1, trackIndex + 1);
    } else if (event.key === "ArrowLeft") {
      nextRound = Math.max(0, roundIndex - 1);
    } else if (event.key === "ArrowRight") {
      nextRound = Math.min(matrixRounds - 1, roundIndex + 1);
    } else {
      return;
    }
    event.preventDefault();
    document
      .querySelector<HTMLButtonElement>(
        `[data-matrix-cell="${nextTrack}-${nextRound}"]`,
      )
      ?.focus();
  }

  const visibleWarnings = infeasible ? data.warnings.slice(1) : data.warnings;
  const warningNeedsAction = visibleWarnings.some(
    (warning) => warning.severity !== "info",
  );

  return (
    <div className="view">
      <PageHeader
        title={viewAsParticipant ? "Redaction preview" : "Schedule planner"}
        description={
          viewAsParticipant
            ? "Only the sample participant's assignments are visible. This is not the participant app."
            : infeasible
              ? `${data.event?.name ?? "Event"}. The matrix shows the assignments that could be placed.`
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
            <strong>Redaction preview, not the participant app.</strong> Only
            this sample participant's assignments are visible. The participant
            app also shows dates.
            <span className="banner__meta">
              Sample participant: {viewer ? viewer.participantName : "none"}.
            </span>
          </div>
        </div>
      ) : null}

      <div
        className={`plan-summary${
          viewAsParticipant ? " plan-summary--participant" : ""
        }`}
      >
        <SummaryFact
          label="Rounds"
          value={`${data.totalRounds} (1 + ${data.exchangeRounds})`}
        />
        <SummaryFact label="Tracks" value={String(data.tracks)} />
        <SummaryFact label="People" value={String(data.participants)} />
        {!viewAsParticipant ? (
          <SummaryFact label="Groups" value={String(data.groups.length)} />
        ) : null}
      </div>

      {!noPlan && !infeasible && !viewAsParticipant && !data.committed ? (
        <section className="plan-actionbar" aria-label="Plan actions">
          <div className="plan-actionbar__copy">
            <div className="plan-actionbar__heading">
              <strong>Draft schedule</strong>
              <span className="card__meta">Plan v{data.version}</span>
              {visibleWarnings.length > 0 ? (
                <>
                  <StatusPill tone={warningNeedsAction ? "warn" : "info"}>
                    {visibleWarnings.length}{" "}
                    {warningNeedsAction
                      ? "to review"
                      : plural(visibleWarnings.length, "note", "notes")}
                  </StatusPill>
                  <a
                    className="warning-jump"
                    href="#warnings-title"
                    onClick={() => {
                      document.getElementById("warnings-title")?.focus();
                    }}
                  >
                    Review warnings
                  </a>
                </>
              ) : null}
            </div>
            <span>
              Locking posts a short checksum in the announcement channel.
              Participants compare the checksum to confirm that the schedule did
              not change. A new schedule version has a new checksum.
            </span>
          </div>
          <div className="plan-actions">
            {data.warnings.some(
              (warning) =>
                warning.id === "group-size-small" ||
                warning.id === "group-size-unequal",
            ) ? (
              <Button
                variant="secondary"
                icon="users"
                onClick={onRepairGroups}
              >
                Repair groups…
              </Button>
            ) : null}
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
                onClick={() => setConfirmRebuild(true)}
              >
                Rebuild draft…
              </Button>
              <span className="field-help">
                Rebuilds from the full roster. Earlier rounds can change. The
                no-repeat rule still applies.
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
                  <div className="matrix-heading">
                    <h2 id="matrix-title">Assignment matrix</h2>
                    <span
                      className="matrix-filter-status"
                      role="status"
                      aria-live="polite"
                    >
                      {!viewAsParticipant && selectedGroup
                        ? `Highlighting Group ${selectedGroup} · ${selectedGroupCount} tracks. Other tracks stay visible.`
                        : ""}
                    </span>
                  </div>
                  <p className="card__subtitle">
                    {viewAsParticipant
                      ? "Only the sample participant's assignments are visible."
                      : "Each cell shows who works on the track in that round. The group label marks the owner group. A participant never gets their own track and never gets the same track twice."}
                  </p>
                  {canEdit ? (
                    <p className="field-help matrix-edit-help">
                      Select a cell to change the holder. The two assignments in
                      that round swap, and the planner validates the result.
                    </p>
                  ) : null}
                  {!viewAsParticipant && data.groups.length > 0 ? (
                    <>
                      <div
                        className="group-filter"
                        role="group"
                        aria-label="Highlight tracks by owner group"
                      >
                        <button
                          type="button"
                          className="chip"
                          aria-pressed={selectedGroup === null}
                          onClick={() => setSelectedGroup(null)}
                        >
                          All groups
                        </button>
                        {data.groups.map((group) => (
                          <button
                            key={group.name}
                            type="button"
                            className="chip"
                            aria-pressed={selectedGroup === group.name}
                            onClick={() => setSelectedGroup(group.name)}
                          >
                            Group {group.name}
                          </button>
                        ))}
                      </div>
                      {data.phases.length > 0 ? (
                        <p className="field-help group-rotation-summary">
                          {data.phases.length} phases: {phaseRanges}.{" "}
                          {fallbackPhases.length > 0
                            ? `Global pool fallback in ${fallbackLabel}.`
                            : "All phases run in groups."}
                        </p>
                      ) : null}
                    </>
                  ) : null}
                  {canEdit ? (
                    <p id="matrix-scroll-help" className="visually-hidden">
                      Scroll both axes. Tab to the first assignment cell. Use
                      the arrow keys to move between cells. Press Enter to edit
                      the focused cell.
                    </p>
                  ) : viewAsParticipant ? (
                    <p
                      id="matrix-scroll-help-readonly"
                      className="visually-hidden"
                    >
                      Scroll both axes. This preview shows only the sample
                      participant's assignments.
                    </p>
                  ) : (
                    <p
                      id="matrix-scroll-help-locked"
                      className="visually-hidden"
                    >
                      Scroll both axes. This schedule is locked. The
                      assignments are read-only.
                    </p>
                  )}
                </div>
              </div>
              <div
                className="seed-band"
                role="group"
                aria-labelledby="seed-band-title"
              >
                <div className="seed-band__header">
                  <h3 id="seed-band-title">R1 · Seed round</h3>
                  <span className="card__meta">Host only</span>
                </div>
                <p className="field-help">
                  Each person owns one seed. Exchanges start at R2 and end at R
                  {data.totalRounds}.
                </p>
                <ul className="seed-band__list" role="list">
                  {data.seedOwners.map((seed) => (
                    <li key={seed.trackCode}>
                      <code>{seed.trackCode}</code>
                      <span>
                        <span className="visually-hidden">owned by </span>
                        {seed.ownerName}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
              <div
                className="matrix-scroll"
                tabIndex={0}
                role="group"
                aria-labelledby="matrix-title"
                aria-describedby={
                  canEdit
                    ? "matrix-scroll-help"
                    : viewAsParticipant
                      ? "matrix-scroll-help-readonly"
                      : "matrix-scroll-help-locked"
                }
                onKeyDown={(event) => {
                  if (!canEdit) {
                    return;
                  }
                  if (
                    event.target === event.currentTarget &&
                    ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(
                      event.key,
                    )
                  ) {
                    event.preventDefault();
                    document
                      .querySelector<HTMLButtonElement>(
                        '[data-matrix-cell="0-0"]',
                      )
                      ?.focus();
                  }
                }}
              >
                <table className="matrix">
                  <caption className="visually-hidden">
                    Track by round assignment matrix
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">Track</th>
                      {Array.from({ length: matrixRounds }, (_, index) => (
                        <th scope="col" key={index}>R{index + 2}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {data.matrix.map((track, trackIndex) => (
                      <tr
                        key={track.trackCode}
                        className={
                          !viewAsParticipant &&
                          selectedGroup &&
                          track.ownerGroup === selectedGroup
                            ? "matrix__row--match"
                            : undefined
                        }
                      >
                        <th scope="row">
                          <code>{track.trackCode}</code>
                          {viewAsParticipant ? null : (
                            <small
                              className="matrix__group-name"
                              title={`Group ${track.ownerGroup}`}
                            >
                              Group {track.ownerGroup}
                            </small>
                          )}
                        </th>
                        {track.cells.map((cell, roundIndex) => {
                          const mine = cell.participantId === viewer?.participantId;
                          return (
                            <td key={cell.round}>
                              {viewAsParticipant ? (
                                mine ? (
                                  <span className="matrix-you">You</span>
                                ) : (
                                  <span className="matrix-blank">
                                    <span aria-hidden="true">—</span>
                                    <span className="visually-hidden">No assignment</span>
                                  </span>
                                )
                              ) : canEdit ? (
                                <button
                                  type="button"
                                  className="matrix-edit"
                                  aria-haspopup="dialog"
                                  data-matrix-cell={`${trackIndex}-${roundIndex}`}
                                  tabIndex={
                                    trackIndex === 0 && roundIndex === 0 ? 0 : -1
                                  }
                                  aria-label={`Edit Round ${cell.round + 2} track ${cell.trackCode}, current holder ${cell.participantName}`}
                                  onKeyDown={(event) =>
                                    moveMatrixFocus(
                                      event,
                                      trackIndex,
                                      roundIndex,
                                    )
                                  }
                                  onClick={() =>
                                    setEditCell({
                                      round: cell.round,
                                      trackCode: cell.trackCode,
                                    })
                                  }
                                >
                                  {shortName(cell.participantName)}
                                  {shortName(cell.participantName) === cell.participantName ? null : (
                                    <span className="visually-hidden">
                                      {" "}
                                      ({cell.participantName})
                                    </span>
                                  )}
                                </button>
                              ) : (
                                <span title={cell.participantName}>
                                  {shortName(cell.participantName)}
                                  {shortName(cell.participantName) === cell.participantName ? null : (
                                    <span className="visually-hidden">
                                      {" "}
                                      ({cell.participantName})
                                    </span>
                                  )}
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
                  <h2 id="graph-title">
                    {data.committed
                      ? `Round ${data.graph.round + 2} graph`
                      : `Preview: round ${data.graph.round + 2}`}
                  </h2>
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
                  <h2 id="warnings-title" tabIndex={-1}>Planner warnings</h2>
                  <StatusPill tone={infeasible ? "danger" : warningNeedsAction ? "warn" : "info"}>
                    {visibleWarnings.length}{" "}
                    {warningNeedsAction
                      ? "to review"
                      : plural(visibleWarnings.length, "note", "notes")}
                  </StatusPill>
                </div>
                <div className="warnings">
                  {visibleWarnings.map((warning) => (
                    <div className={`warning warning--${warning.severity}`} key={warning.id}>
                      <Icon
                        name={
                          warning.severity === "danger"
                            ? "alert"
                            : warning.severity === "warn"
                              ? "clock"
                              : "help"
                        }
                        size={18}
                      />
                      <div>
                        <strong>
                          <span className="visually-hidden">
                            {warning.severity === "danger"
                              ? "Danger: "
                              : warning.severity === "warn"
                                ? "Warning: "
                                : "Info: "}
                          </span>
                          {warning.title}
                        </strong>
                        <p>{warning.detail}</p>
                      </div>
                    </div>
                  ))}
                </div>
              </section>
            ) : null}
          </div>

          {!viewAsParticipant ? (
            <aside className="planner-aside" aria-label="Planner details">
            {!viewAsParticipant ? (
              <section className="card fun-card" aria-labelledby="fun-title">
                <div className="card__header">
                  <div>
                    <h2 id="fun-title">Fun score</h2>
                    <p className="card__subtitle">Host-only planning aid.</p>
                  </div>
                  {data.funScore === null ? (
                    <span className="fun-score fun-score--empty">Not scored</span>
                  ) : (
                    <span className="fun-score">{data.funScore}/100</span>
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
                  {infeasible
                    ? "The planner reports why this round is blocked. No score is available."
                    : "A heuristic for the host. It estimates reach, variety, fairness, and freshness. It does not predict edits."}
                </p>
              </section>
            ) : null}

            {!viewAsParticipant ? (
              <section className="card groups-card" aria-labelledby="groups-title">
                <div className="card__header">
                  <h2 id="groups-title">Groups</h2>
                  <span className="card__meta">{data.groups.length} groups</span>
                </div>
                {data.phases.length > 0 ? (
                  <section className="phase-list" aria-labelledby="phases-title">
                    <h3 id="phases-title">Rotation</h3>
                    <ol>
                      {data.phases.map((phase) => (
                        <li key={phase.index}>
                          <span>
                            Phase {phase.index + 1} · rounds {phase.startRound + 2}
                            –{phase.startRound + phase.rounds + 1}
                          </span>
                          <span
                            className={
                              phase.globalPool
                                ? "phase-list__pool"
                                : "phase-list__local"
                            }
                          >
                            {phase.globalPool
                              ? "Global pool (fallback)"
                              : "In groups"}
                          </span>
                        </li>
                      ))}
                    </ol>
                    <p className="field-help">
                      Phase 1 stays in the base group. Later phases move each
                      person one group forward. No one receives the same track
                      twice.
                    </p>
                  </section>
                ) : null}
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
                      <details className="group-list__tracks">
                        <summary>Track codes</summary>
                        <p>{group.trackCodes.join(", ")}</p>
                      </details>
                    </li>
                  ))}
                </ul>
                <p className="field-help">
                  Participants never see group names or membership.
                </p>
              </section>
            ) : null}

            </aside>
          ) : null}
        </div>
      ) : null}

      {editCell ? (
        <Dialog
          title={`Change Round ${editCell.round + 2} assignment`}
          initialFocusId="edit-holder"
          onClose={() => {
            setEditCell(null);
            setEditError("");
          }}
          footer={
            <>
              <Button
                id="edit-cancel"
                variant="secondary"
                onClick={() => {
                  setEditCell(null);
                  setEditError("");
                }}
              >
                Cancel
              </Button>
              {editCandidates.length === 0 && !editCandidatesLoading ? (
                <Button
                  variant="secondary"
                  icon="refresh"
                  onClick={() => {
                    setEditCell(null);
                    setConfirmReplace(true);
                  }}
                >
                  Replace draft
                </Button>
              ) : null}
              <Button
                variant="primary"
                aria-busy={editBusy}
                disabled={
                  editCandidatesLoading ||
                  !editTarget ||
                  editCandidates.length === 0 ||
                  editCandidate.length === 0
                }
                onClick={async () => {
                  if (!editCell || !editCandidate) {
                    return;
                  }
                  setEditBusy(true);
                  setEditError("");
                  try {
                    await onEditAssignment(
                      editCell.round,
                      editCell.trackCode,
                      editCandidate,
                      data.version,
                    );
                    setEditCell(null);
                  } catch (error) {
                    setEditError(
                      error instanceof Error
                        ? error.message
                        : String(error),
                    );
                  } finally {
                    setEditBusy(false);
                  }
                }}
              >
                {editBusy ? "Saving…" : "Save change"}
              </Button>
            </>
          }
        >
          {editTarget ? (
            <>
              <p>
                Track <code>{editCell.trackCode}</code> in Round{" "}
                {editCell.round + 2} is held by{" "}
                <strong>{editTarget.participantName}</strong>. Choose a new
                holder. The two people swap tracks in this round.
              </p>
              {editCandidatesLoading ? (
                <p className="dialog__meta" role="status">
                  Checking which people can take this track…
                </p>
              ) : editCandidates.length === 0 ? (
                <p className="dialog__meta">
                  No person in this round can take this track without breaking a
                  rule. Replace the draft to change the structure.
                </p>
              ) : (
                <label className="field">
                  <span className="dialog__label">New holder</span>
                  <select
                    id="edit-holder"
                    className="input"
                    value={editCandidate}
                    onChange={(event) =>
                      setEditCandidate(event.target.value)
                    }
                  >
                    <option value="">Choose a person</option>
                    {editCandidates.map((candidate) => (
                      <option
                        key={candidate.participantId}
                        value={candidate.participantId}
                      >
                        {candidate.participantName}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {editCandidateInfo ? (
                <p className="dialog__meta" aria-live="polite">
                  After the swap: {editCandidateInfo.participantName} takes{" "}
                  {editCell.trackCode}; {editTarget.participantName} takes{" "}
                  {editCandidateInfo.currentTrack}.
                </p>
              ) : null}
              <p className="dialog__meta">
                The planner blocks an own track, the same track twice, or two
                people on one track. The change creates plan v
                {data.version + 1}, a new commitment, and an audit entry. No
                offers are sent and no one is marked as seen.
              </p>
              {editBusy ? (
                <p className="visually-hidden" role="status">
                  Saving the assignment.
                </p>
              ) : null}
              {editError ? (
                <p className="dialog__error" role="alert">
                  {editError}
                </p>
              ) : null}
            </>
          ) : (
            <p className="dialog__error" role="alert">
              That assignment is no longer in the plan. Reload the planner.
            </p>
          )}
        </Dialog>
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
            and rules with a new seed, so the matrix can change. Review it
            before you lock.
          </p>
        </Dialog>
      ) : null}

      {confirmRebuild ? (
        <Dialog
          title="Rebuild this draft?"
          onClose={() => setConfirmRebuild(false)}
          footer={
            <>
              <Button
                variant="secondary"
                onClick={() => setConfirmRebuild(false)}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                icon="shuffle"
                onClick={() => {
                  setConfirmRebuild(false);
                  onGeneratePlan();
                }}
              >
                Rebuild draft
              </Button>
            </>
          }
        >
          <p>
            The current draft is discarded. The new draft uses the full roster
            and a new seed, so earlier rounds can change. Review the matrix
            before you lock it.
          </p>
          <p className="field-help">
            The no-repeat and no-own-track rules still apply. No offers are sent
            and no one is marked as having seen a track.
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
            The bot posts a short checksum in the announcement channel.
            Participants compare the checksum to confirm that the schedule did
            not change. A new schedule version has a new checksum.
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
        aria-labelledby="graph-figure-title graph-figure-desc"
        className="plan-graph"
        preserveAspectRatio="xMidYMid meet"
      >
        <title id="graph-figure-title">Round {data.graph.round + 2} assignment graph</title>
        <desc id="graph-figure-desc">
          Tracks on the left connect to the people who hold them in this round.
          The graph does not show owners or group membership.{" "}
          {participantView
            ? "The matrix above shows only the sample participant's assignments."
            : "The assignment matrix above lists every track and person pair."}
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
            <title>{person}</title>
            {participantView ? "You" : shortName(person)}
          </text>
        ))}
        <text x="24" y="16" className="graph-caption">
          Track
        </text>
        <text x="620" y="16" className="graph-caption">
          Person, round {data.graph.round + 2}
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
