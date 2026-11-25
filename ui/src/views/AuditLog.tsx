import { useMemo, useState } from "react";
import type { AuditEntry } from "../types";
import {
  Button,
  EmptyState,
  PageHeader,
  Skeleton,
} from "../components";
import { Icon } from "../icons";

export function AuditLogView({
  entries,
  onExport,
  onNavigate,
}: {
  entries: AuditEntry[];
  onExport: () => void;
  onNavigate: () => void;
}) {
  const [query, setQuery] = useState("");
  const [action, setAction] = useState("all");

  const actions = useMemo(
    () => Array.from(new Set(entries.map((entry) => entry.action))).sort(),
    [entries],
  );

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const matchesAction = action === "all" || entry.action === action;
      const matchesQuery =
        needle.length === 0 ||
        [entry.actor, entry.action, entry.subject, entry.reason, entry.code]
          .join(" ")
          .toLowerCase()
          .includes(needle);
      return matchesAction && matchesQuery;
    });
  }, [action, entries, query]);

  return (
    <div className="view">
      <PageHeader
        eyebrow="Audit log"
        title="Every decision, in order"
        description="Host actions and transitions. Only you can see this before the reveal; the CSV contains names and track codes. Times are Europe/Stockholm. Entries cannot be edited or deleted."
        actions={
          <Button
            variant="secondary"
            icon="download"
            onClick={onExport}
            disabled={entries.length === 0}
          >
            Export CSV
          </Button>
        }
      />

      {entries.length === 0 ? (
        <EmptyState
          icon="audit"
          title="No audit entries yet"
          detail="Host actions and event transitions appear here. Nothing is hidden from the host."
          action={
            <Button variant="secondary" onClick={onNavigate}>
              Open mission control
            </Button>
          }
        />
      ) : (
        <>
          <section className="filter-bar" aria-label="Audit filters">
            <div className="search">
              <Icon name="search" size={18} />
              <label className="visually-hidden" htmlFor="audit-search">
                Search the audit log
              </label>
              <input
                id="audit-search"
                className="input input--search"
                placeholder="Search actor, track, reason, or code"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <div className="select-wrap">
              <label className="visually-hidden" htmlFor="audit-action">
                Filter by action
              </label>
              <select
                id="audit-action"
                className="input"
                value={action}
                onChange={(event) => setAction(event.target.value)}
              >
                <option value="all">All actions</option>
                {actions.map((value) => (
                  <option value={value} key={value}>
                    {value}
                  </option>
                ))}
              </select>
            </div>
            <p className="filter-count" role="status">
              {filtered.length} of {entries.length} entries
            </p>
          </section>

          {filtered.length === 0 ? (
            <EmptyState
              icon="audit"
              title="No entries match this filter"
              detail="Clear the search or choose another action. The full log is still here."
              action={
                <Button
                  variant="secondary"
                  onClick={() => {
                    setQuery("");
                    setAction("all");
                  }}
                >
                  Clear filters
                </Button>
              }
            />
          ) : (
            <div className="table-card">
              <table className="audit-table">
                <caption className="visually-hidden">
                  Audit log entries, newest first
                </caption>
                <thead>
                  <tr>
                    <th scope="col">Time</th>
                    <th scope="col">Actor</th>
                    <th scope="col">Action</th>
                    <th scope="col">Subject</th>
                    <th scope="col" className="col-reason">
                      Reason
                    </th>
                    <th scope="col" className="col-code">
                      Code
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((entry) => (
                    <tr key={entry.id}>
                      <td className="cell-time">
                        <span className="cell-label">Time</span>
                        <time>{entry.at}</time>
                      </td>
                      <td>
                        <span className="cell-label">Actor</span>
                        <span
                          className={
                            entry.actor === "Host"
                              ? "actor actor--host"
                              : entry.actor === "Bot"
                                ? "actor actor--bot"
                                : "actor"
                          }
                        >
                          {entry.actor}
                        </span>
                      </td>
                      <td>
                        <span className="cell-label">Action</span>
                        <code className="action-code">{entry.action}</code>
                      </td>
                      <td>
                        <span className="cell-label">Subject</span>
                        <span className="cell-subject">{entry.subject}</span>
                      </td>
                      <td className="col-reason cell-reason" title={entry.reason}>
                        <span className="cell-label">Reason</span>
                        {entry.reason}
                      </td>
                      <td className="col-code">
                        <span className="cell-label">Code</span>
                        <code className="mono-muted">{entry.code}</code>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}

export function AuditLogSkeleton() {
  return (
    <div
      className="view"
      role="status"
      aria-busy="true"
      aria-label="Loading audit log"
    >
      <span className="visually-hidden">Loading audit log</span>
      <div className="page-header">
        <div className="page-header__copy">
          <Skeleton width="90px" height={12} />
          <Skeleton width="360px" height={44} className="skeleton--title" />
          <Skeleton width="560px" height={14} />
          <Skeleton width="520px" height={14} />
          <Skeleton width="280px" height={14} />
        </div>
      </div>
      <div className="filter-bar skeleton-filter">
        <Skeleton width="100%" height={42} />
        <Skeleton width="100%" height={42} />
        <Skeleton width="80px" height={14} />
      </div>
      <div className="table-card">
        <div className="skeleton-header-row">
          <Skeleton width="60px" height={10} />
          <Skeleton width="44px" height={10} />
          <Skeleton width="72px" height={10} />
          <Skeleton width="54px" height={10} />
          <Skeleton width="54px" height={10} />
          <Skeleton width="38px" height={10} />
        </div>
        <div className="skeleton-table">
          {Array.from({ length: 9 }, (_, index) => (
            <div className="skeleton-row" key={index}>
              <Skeleton width="86px" height={12} />
              <Skeleton width="52px" height={12} />
              <Skeleton width="110px" height={12} />
              <Skeleton width="60px" height={12} />
              <Skeleton width="92%" height={12} />
              <Skeleton width="48px" height={12} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
