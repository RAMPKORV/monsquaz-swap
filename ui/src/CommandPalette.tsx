import { useEffect, useMemo, useRef, useState } from "react";
import type { IconName } from "./icons";
import { Icon } from "./icons";
import type { ViewKey } from "./types";

type Command = {
  id: string;
  label: string;
  hint: string;
  icon: IconName;
  aliases?: string[];
  run: () => void;
};

export function CommandPalette({
  open,
  onClose,
  onNavigate,
  onAction,
  onToggleTheme,
}: {
  open: boolean;
  onClose: () => void;
  onNavigate: (view: ViewKey) => void;
  onAction: (label: string) => void;
  onToggleTheme: () => void;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const paletteRef = useRef<HTMLDivElement | null>(null);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const commands = useMemo<Command[]>(
    () => [
      { id: "mission", label: "Go to mission control", hint: "Overview", icon: "compass", run: () => onNavigate("mission") },
      { id: "setup", label: "Go to event setup", hint: "Event tools", icon: "plus", run: () => onNavigate("setup") },
      { id: "people", label: "Go to participants", hint: "Event tools", icon: "users", aliases: ["people", "roster"], run: () => onNavigate("people") },
      { id: "plan", label: "Go to schedule planner", hint: "Event tools", icon: "shuffle", run: () => onNavigate("plan") },
      { id: "audit", label: "Go to audit log", hint: "Records", icon: "audit", run: () => onNavigate("audit") },
      { id: "settings", label: "Go to settings", hint: "Connection and storage", icon: "settings", aliases: ["notifications", "appearance", "theme", "retention"], run: () => onNavigate("settings") },
      { id: "theme", label: "Toggle light and dark theme", hint: "Appearance", icon: "sun", run: onToggleTheme },
      { id: "connection", label: "Test the bot connection", hint: "Host action", icon: "refresh", run: () => onAction("Test connection") },
    ],
    [onAction, onNavigate, onToggleTheme],
  );

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) {
      return commands;
    }
    return commands.filter((command) =>
      `${command.label} ${command.hint} ${(command.aliases ?? []).join(" ")}`
        .toLowerCase()
        .includes(needle),
    );
  }, [commands, query]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const previous = document.activeElement as HTMLElement | null;
    return () => previous?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    setQuery("");
    setActive(0);
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "Tab") {
        event.preventDefault();
        inputRef.current?.focus();
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        setActive((value) => Math.min(value + 1, Math.max(0, filtered.length - 1)));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setActive((value) => Math.max(0, value - 1));
      } else if (event.key === "Enter") {
        const command = filtered[active];
        if (command) {
          event.preventDefault();
          command.run();
          onClose();
        }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [active, filtered, onClose, open]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="palette-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        ref={paletteRef}
      >
        <div className="palette__search">
          <Icon name="search" size={17} />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(0);
            }}
            placeholder="Jump to a view or run a host action"
            aria-label="Search commands"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded="true"
            aria-controls="palette-listbox"
            aria-activedescendant={
              filtered[active] ? `palette-option-${filtered[active].id}` : undefined
            }
          />
          <kbd>Esc</kbd>
        </div>
        <ul
          className="palette__list"
          id="palette-listbox"
          role="listbox"
          aria-label="Commands"
        >
          {filtered.map((command, index) => (
            <li key={command.id} role="presentation">
              <button
                type="button"
                id={`palette-option-${command.id}`}
                role="option"
                tabIndex={-1}
                aria-selected={index === active}
                className={`palette__item ${index === active ? "palette__item--active" : ""}`}
                onMouseEnter={() => setActive(index)}
                onFocus={() => setActive(index)}
                onClick={() => {
                  command.run();
                  onClose();
                }}
              >
                <Icon name={command.icon} size={16} />
                <span>{command.label}</span>
                <small>{command.hint}</small>
              </button>
            </li>
          ))}
        </ul>
        {filtered.length === 0 ? (
          <p className="palette__empty" role="status">
            No commands match “{query}”.
          </p>
        ) : null}
      </div>
    </div>
  );
}
