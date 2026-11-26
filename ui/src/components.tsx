import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Icon, type IconName } from "./icons";

type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";

type ButtonProps = {
  children: ReactNode;
  id?: string;
  variant?: ButtonVariant;
  icon?: IconName;
  onClick?: () => void;
  disabled?: boolean;
  type?: "button" | "submit";
  className?: string;
  ariaLabel?: string;
  describedBy?: string;
};

export function Button({
  children,
  id,
  variant = "secondary",
  icon,
  onClick,
  disabled,
  type = "button",
  className = "",
  ariaLabel,
  describedBy,
}: ButtonProps) {
  return (
    <button
      id={id}
      type={type}
      className={`button button--${variant} ${className}`.trim()}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      aria-describedby={describedBy}
    >
      {icon ? <Icon name={icon} size={18} /> : null}
      <span>{children}</span>
    </button>
  );
}

type StatusTone = "ok" | "warn" | "danger" | "muted" | "info";

export function StatusPill({
  tone,
  children,
  icon,
}: {
  tone: StatusTone;
  children: ReactNode;
  icon?: IconName;
}) {
  const fallback: IconName | undefined =
    tone === "ok"
      ? "check"
      : tone === "warn" || tone === "danger"
        ? "alert"
        : tone === "info"
          ? "clock"
          : undefined;
  const mark = icon ?? fallback;
  return (
    <span className={`status status--${tone}`}>
      {mark ? <Icon name={mark} size={13} /> : null}
      <span>{children}</span>
    </span>
  );
}

export function PageHeader({
  title,
  description,
  actions,
  titleId,
}: {
  eyebrow?: string;
  title: string;
  description: string;
  actions?: ReactNode;
  titleId?: string;
}) {
  return (
    <header className="page-header">
      <div className="page-header__copy">
        <h1 id={titleId} tabIndex={-1}>
          {title}
        </h1>
        <p className="page-header__description">{description}</p>
      </div>
      {actions ? <div className="page-header__actions">{actions}</div> : null}
    </header>
  );
}

export function Meter({
  value,
  max,
  label,
}: {
  value: number;
  max: number;
  label: string;
}) {
  const percent = max > 0 ? Math.min(100, Math.round((value / max) * 100)) : 0;
  return (
    <div
      className="meter"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={`${value} of ${max}`}
    >
      <span className="meter__fill" style={{ width: `${percent}%` }} />
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  description: string;
}) {
  const descriptionId = useId();
  return (
    <div className="toggle-row">
      <div className="toggle-row__copy">
        <span className="field-label">{label}</span>
        <span className="field-help" id={descriptionId}>
          {description}
        </span>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        aria-describedby={descriptionId}
        className={`toggle ${checked ? "toggle--on" : ""}`}
        onClick={() => onChange(!checked)}
      >
        <span className="toggle__thumb" />
      </button>
    </div>
  );
}

export function Skeleton({
  width,
  height = 16,
  className = "",
}: {
  width?: string;
  height?: number;
  className?: string;
}) {
  return (
    <span
      className={`skeleton ${className}`.trim()}
      style={{ width, height }}
      aria-hidden="true"
    />
  );
}

export function EmptyState({
  icon,
  title,
  detail,
  action,
}: {
  icon: IconName;
  title: string;
  detail: string;
  action?: ReactNode;
}) {
  return (
    <div className="state-panel">
      <span className="state-panel__icon">
        <Icon name={icon} size={28} />
      </span>
      <h2 tabIndex={-1}>{title}</h2>
      <p>{detail}</p>
      {action ? <div className="state-panel__action">{action}</div> : null}
    </div>
  );
}

export function ErrorState({
  title = "The local service did not answer",
  detail = "Your event data is safe. Retry the request. If it fails again, restart the app. Then copy the error code.",
  code = "MSW-UI-001",
  onRetry,
}: {
  title?: string;
  detail?: string;
  code?: string;
  onRetry: () => void;
}) {
  const [copied, setCopied] = useState(false);

  async function copyCode() {
    if (!navigator.clipboard) {
      return;
    }
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="state-panel state-panel--error" role="alert">
      <span className="state-panel__icon">
        <Icon name="alert" size={28} />
      </span>
      <h1 tabIndex={-1}>{title}</h1>
      <p>{detail}</p>
      <p className="error-code">
        Error code <code>{code}</code>
        <button type="button" className="link-button" onClick={copyCode}>
          {copied ? "Copied" : "Copy code"}
        </button>
      </p>
      <div className="state-panel__action">
        <Button variant="primary" icon="refresh" onClick={onRetry}>
          Retry
        </Button>
      </div>
    </div>
  );
}

export function Dialog({
  title,
  children,
  onClose,
  footer,
  labelledBy = "dialog-title",
  initialFocusId,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  footer: ReactNode;
  labelledBy?: string;
  initialFocusId?: string;
}) {
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const initial =
      (initialFocusId
        ? document.getElementById(initialFocusId)
        : null) ??
      dialogRef.current?.querySelector<HTMLElement>(
        "input:not([disabled]), select:not([disabled]), textarea:not([disabled])",
      ) ??
      closeRef.current;
    initial?.focus();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) {
        return;
      }
      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href]',
        ),
      );
      if (focusable.length === 0) {
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus();
    };
  }, []);

  return (
    <div className="dialog-backdrop" role="presentation">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={`${labelledBy}-body`}
        ref={dialogRef}
      >
        <div className="dialog__header">
          <h2 id={labelledBy}>{title}</h2>
          <button
            type="button"
            className="icon-button"
            aria-label="Close dialog"
            onClick={onClose}
            ref={closeRef}
          >
            <Icon name="x" size={18} />
          </button>
        </div>
        <div className="dialog__body" id={`${labelledBy}-body`}>
          {children}
        </div>
        <div className="dialog__footer">{footer}</div>
      </div>
    </div>
  );
}

export function Field({
  label,
  help,
  helpId,
  children,
  htmlFor,
}: {
  label: string;
  help?: string;
  helpId?: string;
  children: ReactNode;
  htmlFor: string;
}) {
  return (
    <div className="field">
      <label className="field-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
      {help ? (
        <p className="field-help" id={helpId}>
          {help}
        </p>
      ) : null}
    </div>
  );
}
