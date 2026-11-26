import { useEffect, useMemo, useState } from "react";
import type { PeopleData, PersonRow } from "../types";
import { Button, Dialog, EmptyState, Field, PageHeader, Skeleton, StatusPill } from "../components";
import { Icon } from "../icons";

export function PeopleView({
  data,
  onAction,
  onSetPaused,
  onAddAvailability,
  onRemoveAvailability,
  timezone,
}: {
  data: PeopleData;
  onAction: (label: string) => void;
  onSetPaused: (userId: string, paused: boolean) => void;
  onAddAvailability: (
    participantId: string,
    startsAt: number,
    endsAt: number,
    note: string,
  ) => void;
  onRemoveAvailability: (id: string) => void;
  timezone?: string;
}) {
  const [rows, setRows] = useState<PersonRow[]>(data.rows);
  const [query, setQuery] = useState("");
  const [role, setRole] = useState("all");
  const [status, setStatus] = useState("all");
  const [awayFor, setAwayFor] = useState<PersonRow | null>(() =>
    new URLSearchParams(window.location.search).get("dialog") === "away"
      ? (data.rows[0] ?? null)
      : null,
  );
  const [awayStart, setAwayStart] = useState(() => {
    const date = new Date(Date.now() + 86_400_000);
    const offset = date.getTimezoneOffset() * 60_000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 16);
  });
  const [awayEnd, setAwayEnd] = useState(() => {
    const date = new Date(Date.now() + 2 * 86_400_000);
    const offset = date.getTimezoneOffset() * 60_000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 16);
  });
  const [awayNote, setAwayNote] = useState("");
  const [awayError, setAwayError] = useState<string | null>(null);

  useEffect(() => {
    setRows(data.rows);
  }, [data]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return rows.filter((row) => {
      const matchesQuery =
        needle.length === 0 ||
        [
          row.displayName,
          row.id,
          row.seedTrack ?? "",
          row.timezone,
          row.deliveryRisk ? "delivery risk" : "",
        ]
          .join(" ")
          .toLowerCase()
          .includes(needle);
      const matchesRole = role === "all" || row.role === role;
      const matchesStatus = status === "all" || row.status === status;
      return matchesQuery && matchesRole && matchesStatus;
    });
  }, [query, role, rows, status]);

  const ready = rows.filter((row) => row.status === "Ready").length;
  const fillIns = rows.filter((row) => row.role === "Fill-in").length;
  const paused = rows.filter((row) => row.status === "Paused").length;
  const risk = rows.filter((row) => row.deliveryRisk).length;
  const riskRow = rows.find((row) => row.deliveryRisk);
  const awayStartAt = Math.floor(new Date(awayStart).getTime() / 1000);
  const awayEndAt = Math.floor(new Date(awayEnd).getTime() / 1000);
  const awayValid =
    Number.isFinite(awayStartAt) &&
    Number.isFinite(awayEndAt) &&
    awayStartAt < awayEndAt;

  if (!data.event) {
    return (
      <div className="view">
        <PageHeader
          title="Participants"
          description="Each seeded participant owns one anonymous track. Fill-ins join without a personal track."
        />
        <EmptyState
          icon="users"
          title="No event yet"
          detail="Create an event first. People then join by sending one seed track to the bot."
          action={
            <Button variant="secondary" onClick={() => onAction("Open event setup")}>
              Open event setup
            </Button>
          }
        />
      </div>
    );
  }

  return (
    <div className="view">
      <PageHeader
        title="Participants"
        description={`${data.event.name}. ${
          data.event.openSchedule
            ? "Open schedule is on. Participants also see the full mapping."
            : "The host sees the mapping; participants do not."
        }`}
        actions={
          <StatusPill tone={paused > 0 ? "warn" : "ok"}>
            {paused > 0 ? `${paused} paused` : "All ready"}
          </StatusPill>
        }
      />

      {data.event.openSchedule ? (
        <div className="banner banner--warn" role="status">
          <Icon name="eye-off" size={20} />
          <div>
            <strong>Open schedule is on.</strong> Participants also see the full
            matrix, seed owners, group names, editor names, titles, and comments.
          </div>
        </div>
      ) : null}

      <div className="people-summary">
        <Fact label="Ready" value={ready} icon="users" />
        <Fact
          label="Seeds ready"
          value={
            rows.filter(
              (row) => row.role === "Seed" && row.status === "Ready",
            ).length
          }
          icon="disc"
        />
        <Fact label="Fill-ins" value={fillIns} icon="shuffle" />
        <Fact
          label="Paused"
          value={paused}
          icon="alert"
          tone={paused > 0 ? "warn" : "ok"}
        />
      </div>

      {risk > 0 ? (
        <p className="delivery-note">
          <Icon name="alert" size={16} />
          {riskRow?.displayName ?? "One participant"} did not get the join DM.
          Review the fallback in settings.
          <button
            type="button"
            className="link-button"
            onClick={() => onAction("Open settings")}
          >
            Open settings
          </button>
        </p>
      ) : null}

      <section className="filter-bar" aria-label="Participant filters">
        <div className="search">
          <Icon name="search" size={18} />
          <label className="visually-hidden" htmlFor="people-search">
            Search participants
          </label>
          <input
            id="people-search"
            className="input input--search"
            placeholder="Search a person, track, or timezone"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <div className="select-wrap">
          <label className="visually-hidden" htmlFor="people-role">
            Filter by role
          </label>
          <select
            id="people-role"
            className="input"
            value={role}
            onChange={(event) => setRole(event.target.value)}
          >
            <option value="all">All roles</option>
            <option value="Seed">Seeds</option>
            <option value="Fill-in">Fill-ins</option>
          </select>
        </div>
        <div className="select-wrap">
          <label className="visually-hidden" htmlFor="people-status">
            Filter by status
          </label>
          <select
            id="people-status"
            className="input"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <option value="all">All statuses</option>
            <option value="Ready">Ready</option>
            <option value="Paused">Paused</option>
            <option value="Left">Left</option>
          </select>
        </div>
        <p className="filter-count" role="status">
          {filtered.length} of {rows.length}
          {filtered.length > 16 ? ", scroll for more" : ""}
        </p>
      </section>

      {filtered.length === 0 ? (
        <EmptyState
          icon="users"
          title={rows.length === 0 ? "Nobody has seeded a track yet" : "No people match this filter"}
          detail={
            rows.length === 0
              ? "The doors are open. Share the join instructions. A signup without a seed becomes a fill-in when signups close."
              : "Clear the search or choose another role or status."
          }
          action={
            rows.length > 0 ? (
              <Button
                variant="secondary"
                onClick={() => {
                  setQuery("");
                  setRole("all");
                  setStatus("all");
                }}
              >
                Clear filters
              </Button>
            ) : undefined
          }
        />
      ) : (
        <>
        <div
          className={`table-card people-table-card ${
            filtered.length > 16 ? "table-card--bounded" : ""
          }`}
          tabIndex={filtered.length > 16 ? 0 : undefined}
          aria-label={
            filtered.length > 16
              ? "Participant table, scroll for more rows"
              : undefined
          }
        >
          <table className="people-table">
            <caption className="visually-hidden">
              Participants, roles, status, seed track, timezone, and edit limit
            </caption>
            <thead>
              <tr>
                <th scope="col">Person</th>
                <th scope="col">Role</th>
                <th scope="col">Status</th>
                <th scope="col">Seed track</th>
                <th scope="col" className="col-timezone">Timezone</th>
                <th
                  scope="col"
                  className="col-handoffs"
                  title="This person's handoff cap, compared with the number of exchange rounds. A lower cap blocks full coverage."
                >
                  Cap / rounds
                  <span className="visually-hidden">
                    : handoff cap compared with exchange rounds.
                  </span>
                </th>
                <th scope="col" className="col-joined">Joined</th>
                <th scope="col" className="col-action">Action</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((row) => (
                <tr key={row.id}>
                  <td>
                    <span className="cell-label">Person</span>
                    <span className="person-cell">
                      <span>
                        <span className="person-name">
                          <strong>{row.displayName}</strong>
                          {row.deliveryRisk ? (
                            <span
                              className="risk-mark"
                              title="The bot could not send a direct message."
                            >
                              <Icon name="alert" size={13} />
                              <span className="visually-hidden">
                                Message failure
                              </span>
                            </span>
                          ) : null}
                        </span>
                        <small>{row.id}</small>
                        <small className="person-meta">
                          {row.timezone}, {row.joined}
                        </small>
                        <small className="person-meta">
                          Handoff cap {row.maxHandoffs}
                        </small>
                        {row.away.length > 0 ? (
                          <small className="person-meta person-away">
                            <Icon name="calendar" size={13} /> Away{" "}
                            {formatAwayShort(
                              row.away[0].startsAt,
                              row.away[0].endsAt,
                              timezone,
                            )}
                            {row.away.length > 1
                              ? ` +${row.away.length - 1}`
                              : ""}
                          </small>
                        ) : null}
                      </span>
                    </span>
                  </td>
                  <td>
                    <span className="cell-label">Role</span>
                    <span className={`role role--${row.role === "Seed" ? "seed" : "fill"}`}>
                      {row.role}
                    </span>
                  </td>
                  <td>
                    <span className="cell-label">Status</span>
                    <StatusPill
                      tone={
                        row.status === "Ready"
                          ? "ok"
                          : row.status === "Paused"
                            ? "warn"
                            : "muted"
                      }
                    >
                      {row.status}
                    </StatusPill>
                  </td>
                  <td>
                    <span className="cell-label">Seed track</span>
                    {row.seedTrack ? (
                      <code className="track-code">{row.seedTrack}</code>
                    ) : (
                      <span className="muted">No personal track</span>
                    )}
                  </td>
                  <td className="col-timezone">
                    <span className="cell-label">Timezone</span>
                    {row.timezone}
                  </td>
                  <td className="col-handoffs">
                    <span className="cell-label">Cap / rounds</span>
                    {row.maxHandoffs} / {data.event?.exchangeRounds ?? 0}
                  </td>
                  <td className="col-joined">
                    <span className="cell-label">Joined</span>
                    <time>{row.joined}</time>
                  </td>
                  <td>
                    <span className="cell-label">Action</span>
                    {row.status === "Left" ? (
                      <span className="muted">No action</span>
                    ) : (
                      <div className="header-actions">
                        <Button
                          variant="quiet"
                          ariaLabel={`Set away dates for ${row.displayName}`}
                          onClick={() => {
                            const now = new Date(Date.now() + 86_400_000);
                            const offset = now.getTimezoneOffset() * 60_000;
                            setAwayFor(row);
                            setAwayStart(
                              new Date(now.getTime() - offset)
                                .toISOString()
                                .slice(0, 16),
                            );
                            setAwayEnd(
                              new Date(now.getTime() + 86_400_000 - offset)
                                .toISOString()
                                .slice(0, 16),
                            );
                            setAwayNote("");
                          }}
                        >
                          Away dates
                        </Button>
                        <Button
                          variant="quiet"
                          ariaLabel={`${
                            row.status === "Paused" ? "Resume" : "Pause"
                          } ${row.displayName}`}
                          onClick={() => {
                            setRows((current) =>
                              current.map((person) =>
                                person.id === row.id
                                  ? {
                                      ...person,
                                      status:
                                        person.status === "Paused"
                                          ? "Ready"
                                          : "Paused",
                                    }
                                  : person,
                              ),
                            );
                            onSetPaused(row.id, row.status !== "Paused");
                            if (status !== "all") {
                              window.requestAnimationFrame(() =>
                                document
                                  .getElementById("people-status")
                                  ?.focus(),
                              );
                            }
                          }}
                        >
                          {row.status === "Paused" ? "Resume" : "Pause"}
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        </>
      )}

      {awayFor ? (
        <Dialog
          title={`Away dates for ${awayFor.displayName}`}
          onClose={() => setAwayFor(null)}
          footer={
            <>
              <Button variant="secondary" onClick={() => setAwayFor(null)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                disabled={!awayValid}
                onClick={() => {
                  if (!awayValid) {
                    setAwayError("Away until must be after Away from.");
                    document.getElementById("away-start")?.focus();
                    return;
                  }
                  setAwayError(null);
                  onAddAvailability(
                    awayFor.id,
                    awayStartAt,
                    awayEndAt,
                    awayNote.trim(),
                  );
                  setAwayFor(null);
                }}
              >
                Add away range
              </Button>
            </>
          }
        >
          <p>
            {data.event?.handoffPolicy === "Strict"
              ? "Full coverage cannot skip a round. This range removes the participant from each exchange round that overlaps it, so the schedule is blocked until you remove the range or switch Participation to Cap-limited."
              : "The planner skips this person in the exchange rounds that overlap an away range. Each empty round needs a fill-in or a manual edit. The seed round is not affected."}{" "}
            Times use the event timezone. The database stores UTC.
          </p>
          {awayFor.away.length > 0 ? (
            <ul className="away-list" role="list">
              {awayFor.away.map((range) => (
                <li key={range.id}>
                  <span>
                    {formatAwayRange(range.startsAt, range.endsAt, timezone)}
                    {range.note ? ` · ${range.note}` : ""}
                  </span>
                  <Button
                    variant="quiet"
                    ariaLabel={`Remove away range for ${awayFor.displayName}`}
                    onClick={() => onRemoveAvailability(range.id)}
                  >
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          ) : null}
          {awayError ? (
            <p className="field-help field-help--danger" role="alert">
              {awayError}
            </p>
          ) : null}
          <Field
            label={`Away from${timezone ? ` (${timezone})` : ""}`}
            htmlFor="away-start"
            helpId="away-start-help"
            help="Times use the event timezone. The database stores UTC. The range ends at the first available time."
          >
            <input
              id="away-start"
              className="input"
              type="datetime-local"
              aria-describedby="away-start-help"
              aria-invalid={awayError ? true : undefined}
              value={awayStart}
              onChange={(event) => setAwayStart(event.target.value)}
            />
          </Field>
          <Field
            label={`Away until${timezone ? ` (${timezone})` : ""}`}
            htmlFor="away-end"
            helpId="away-end-help"
            help="Away until must be after Away from."
          >
            <input
              id="away-end"
              className="input"
              type="datetime-local"
              aria-describedby="away-end-help"
              aria-invalid={awayError ? true : undefined}
              value={awayEnd}
              onChange={(event) => setAwayEnd(event.target.value)}
            />
          </Field>
          <Field label="Note" htmlFor="away-note">
            <input
              id="away-note"
              className="input"
              value={awayNote}
              onChange={(event) => setAwayNote(event.target.value)}
              placeholder="Travel, work trip"
            />
          </Field>
        </Dialog>
      ) : null}
    </div>
  );
}

function formatAwayShort(
  startsAt: number,
  endsAt: number,
  timezone?: string,
) {
  const day = new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "short",
    timeZone: timezone,
  });
  return `${day.format(new Date(startsAt * 1000))}–${day.format(
    new Date(endsAt * 1000),
  )}`;
}

function formatAwayRange(
  startsAt: number,
  endsAt: number,
  timezone?: string,
) {
  const format = new Intl.DateTimeFormat("en-GB", {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: timezone,
  });
  return `${format.format(new Date(startsAt * 1000))} to ${format.format(
    new Date(endsAt * 1000),
  )}${timezone ? ` (${timezone})` : ""}`;
}

function Fact({
  label,
  value,
  icon,
  tone = "ok",
  title,
}: {
  label: string;
  value: number;
  icon: "users" | "disc" | "shuffle" | "alert";
  tone?: "ok" | "warn";
  title?: string;
}) {
  return (
    <div className={`fact ${tone === "warn" ? "fact--warn" : ""}`} title={title}>
      <Icon name={icon} size={18} />
      <span>
        <strong>{value}</strong>
        <small>{label}</small>
      </span>
    </div>
  );
}

export function PeopleSkeleton() {
  return (
    <div className="view" role="status" aria-busy="true" aria-label="Loading participants">
      <span className="visually-hidden">Loading participants</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="110px" height={12} />
          <Skeleton width="420px" height={44} className="skeleton--title" />
          <Skeleton width="min(100%, 560px)" height={16} />
        </div>
      </div>
      <div className="people-summary">
        {Array.from({ length: 4 }, (_, index) => (
          <Skeleton key={index} width="100%" height={76} />
        ))}
      </div>
      <div className="filter-bar">
        <Skeleton width="100%" height={42} />
      </div>
      <div className="table-card">
        <div className="skeleton-table">
          {Array.from({ length: 8 }, (_, index) => (
            <div className="skeleton-row" key={index}>
              <Skeleton width="160px" height={20} />
              <Skeleton width="64px" height={14} />
              <Skeleton width="72px" height={22} />
              <Skeleton width="70px" height={14} />
              <Skeleton width="100px" height={14} />
              <Skeleton width="50px" height={14} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
