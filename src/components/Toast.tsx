import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import "./toast.css";

interface ToastProps {
  kind: "success" | "error";
  message: string;
  details?: string[];
  onClose?: () => void;
  resetKey?: unknown;
  durationMs?: number | null;
}

let region: HTMLDivElement | null = null;
const owners = new WeakMap<HTMLDivElement, number>();

function acquireRegion() {
  if (!region?.isConnected) {
    region = document.createElement("div");
    region.className = "toast-region";
    region.setAttribute("role", "region");
    region.setAttribute("aria-label", "操作提示");
    document.body.append(region);
  }
  owners.set(region, (owners.get(region) ?? 0) + 1);
  return region;
}

function releaseRegion(target: HTMLDivElement) {
  const remaining = (owners.get(target) ?? 1) - 1;
  owners.set(target, remaining);
  if (remaining === 0) {
    target.remove();
    if (region === target) region = null;
  }
}

export default function Toast({ kind, message, details, onClose, resetKey, durationMs = 3000 }: ToastProps) {
  const notification = useMemo(() => ({}), [kind, message, resetKey]);
  const [dismissed, setDismissed] = useState<object | null>(null);
  const [target, setTarget] = useState<HTMLDivElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const currentNotification = useRef(notification);
  currentNotification.current = notification;
  const closedNotification = useRef<object | null>(null);
  const visible = dismissed !== notification;

  const close = useCallback(() => {
    if (currentNotification.current !== notification || closedNotification.current === notification) return;
    closedNotification.current = notification;
    setDismissed(notification);
    onCloseRef.current?.();
  }, [notification]);

  useEffect(() => {
    if (!visible) return;
    const portalTarget = acquireRegion();
    setTarget(portalTarget);
    return () => releaseRegion(portalTarget);
  }, [visible]);

  useEffect(() => {
    if (!visible || durationMs === null) return;
    const timer = window.setTimeout(close, durationMs);
    return () => window.clearTimeout(timer);
  }, [close, visible, durationMs]);

  if (!visible || !target) return null;

  return createPortal(
    <div className={`toast toast-${kind}`} role={kind === "success" ? "status" : "alert"} aria-atomic="true">
      <svg className="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        {kind === "success" ? <path d="m8 12 2.5 2.5L16 9" /> : <path d="m9 9 6 6m0-6-6 6" />}
      </svg>
      <div className="toast-content">
        <p className="toast-message">{message}</p>
        {details?.length ? <ul className="toast-details">{details.map((detail, index) => <li key={index}>{detail}</li>)}</ul> : null}
      </div>
      <button type="button" className="toast-close" aria-label="关闭提示" onClick={close}>
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg>
      </button>
    </div>,
    target,
  );
}
