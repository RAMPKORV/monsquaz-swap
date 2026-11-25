import { useEffect, useMemo, useState } from "react";
import type { PeopleData, PersonRow } from "../types";
import { Button, EmptyState, PageHeader, Skeleton, StatusPill } from "../components";
import { Icon } from "../icons";

export function PeopleView({
  data,
  onAction,
  onSetPaused,
}: {
  data: PeopleData;
  onAction: (label: string) => void;
  onSetPaused: (userId: string, paused: boolean) => void;
}) {
  const [rows, setRows] = useState<PersonRow[]>(data.rows);
  const [query, setQuery] = useState("");
  const [role, setRole] = useState("all");
  const [status, setStatus] = useState("all");

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

  const active = rows.filter((row) => row.status !== "Left").length;
  const fillIns = rows.filter((row) => row.role === "Fill-in").length;
  const paused = rows.filter((row) => row.status === "Paused").length;
  const risk = rows.filter((row) => row.deliveryRisk).length;

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
        description={`${data.event.name}. The host sees the mapping; participants do not.`}
        actions={
          <StatusPill tone={paused > 0 ? "warn" : "ok"}>
            {paused > 0 ? `${paused} paused` : "All ready"}
          </StatusPill>
        }
      />

      <div className="people-summary">
        <Fact label="Active" value={active} icon="users" />
        <Fact
          label="Active seeds"
          value={
            rows.filter(
              (row) => row.role === "Seed" && row.status !== "Left",
            ).length
          }
          icon="disc"
        />
        <Fact label="Fill-ins" value={fillIns} icon="shuffle" />
        <Fact
          label="Message failure"
          value={risk}
          icon="alert"
          tone={risk > 0 ? "warn" : "ok"}
          title="The bot could not send a direct message to this person. Review their delivery settings."
        />
      </div>

      {risk > 0 ? (
        <p className="delivery-note">
          <Icon name="alert" size={16} />
          {risk} {risk === 1 ? "message failure" : "message failures"}: the bot
          could not send a direct message. Review the fallback in settings.
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
                  title="The most tracks this person may edit in the whole event. Fill-in edits count."
                >
                  Edit limit
                  <span className="visually-hidden">
                    : the most tracks this person may edit in the whole event.
                    Fill-in edits count.
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
                    <span className="cell-label">Edit limit</span>
                    {row.maxHandoffs}
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
                                      person.status === "Paused" ? "Ready" : "Paused",
                                  }
                                : person,
                            ),
                          );
                          onSetPaused(row.id, row.status !== "Paused");
                          if (status !== "all") {
                            window.requestAnimationFrame(() =>
                              document.getElementById("people-status")?.focus(),
                            );
                          }
                        }}
                      >
                        {row.status === "Paused" ? "Resume" : "Pause"}
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        </>
      )}
    </div>
  );
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
