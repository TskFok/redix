import { useCallback, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

import "./confirmDialog.css";

interface ConfirmationOptions {
  title?: string;
  confirmLabel?: string;
  danger?: boolean;
}

interface Confirmation {
  message: string;
  options: ConfirmationOptions;
  scope: object;
  resolve: (accepted: boolean) => void;
  returnFocus: HTMLElement | null;
}

export function useConfirmDialog(scope: string): {
  confirm: (message: string, options?: ConfirmationOptions) => Promise<boolean>;
  confirmationDialog: ReactNode;
} {
  const id = useId();
  // A fresh token also invalidates callbacks when returning to an earlier scope.
  const scopeToken = useMemo(() => ({ scope }), [scope]);
  const activeScope = useRef<object | null>(null);
  const pendingRef = useRef<Confirmation | null>(null);
  const [pending, setPending] = useState<Confirmation | null>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const acceptRef = useRef<HTMLButtonElement>(null);

  useLayoutEffect(() => {
    activeScope.current = scopeToken;
    return () => {
      activeScope.current = null;
      const request = pendingRef.current;
      pendingRef.current = null;
      request?.resolve(false);
    };
  }, [scopeToken]);

  const confirm = useCallback((message: string, options: ConfirmationOptions = {}): Promise<boolean> => {
    if (activeScope.current !== scopeToken || pendingRef.current) return Promise.resolve(false);
    return new Promise((resolve) => {
      const request: Confirmation = {
        message,
        options,
        scope: scopeToken,
        resolve,
        returnFocus: document.activeElement instanceof HTMLElement ? document.activeElement : null,
      };
      pendingRef.current = request;
      setPending(request);
    });
  }, [scopeToken]);

  const finish = useCallback((request: Confirmation, accepted: boolean) => {
    if (pendingRef.current !== request) return;
    pendingRef.current = null;
    setPending(null);
    request.resolve(accepted && activeScope.current === request.scope);
  }, []);

  useLayoutEffect(() => {
    if (!pending || pending.scope !== scopeToken) return;
    cancelRef.current?.focus({ preventScroll: true });
    const keydown = (event: KeyboardEvent) => {
      event.stopPropagation();
      if (event.key === "Escape") {
        event.preventDefault();
        finish(pending, false);
      } else if (event.key === "Tab") {
        event.preventDefault();
        const first = cancelRef.current;
        const last = acceptRef.current;
        const target = document.activeElement === first ? last
          : document.activeElement === last ? first : event.shiftKey ? last : first;
        target?.focus({ preventScroll: true });
      }
    };
    const keyup = (event: KeyboardEvent) => event.stopPropagation();
    document.addEventListener("keydown", keydown, true);
    document.addEventListener("keyup", keyup, true);
    return () => {
      document.removeEventListener("keydown", keydown, true);
      document.removeEventListener("keyup", keyup, true);
      if (pending.returnFocus?.isConnected) pending.returnFocus.focus({ preventScroll: true });
    };
  }, [pending, scopeToken, finish]);

  const confirmationDialog = pending && pending.scope === scopeToken ? createPortal(
    <div className="confirm-dialog-backdrop" onClick={(event) => {
      event.stopPropagation();
      if (event.target === event.currentTarget) finish(pending, false);
    }}>
      <section
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-message`}
      >
        <h2 id={`${id}-title`}>{pending.options.title ?? "确认删除"}</h2>
        <p id={`${id}-message`}>{pending.message}</p>
        <div className="confirm-dialog-actions">
          <button ref={cancelRef} className="button button-secondary" type="button" onClick={() => finish(pending, false)}>取消</button>
          <button ref={acceptRef} className={`button ${pending.options.danger === false ? "button-primary" : "button-danger"}`} type="button" onClick={() => finish(pending, true)}>{pending.options.confirmLabel ?? "确认删除"}</button>
        </div>
      </section>
    </div>, document.body,
  ) : null;

  return { confirm, confirmationDialog };
}
