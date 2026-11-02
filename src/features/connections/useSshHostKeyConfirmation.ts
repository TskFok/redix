import { useCallback, useLayoutEffect, useRef } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import { discardSshHostKey, getSshHostKeyChallenge, trustSshHostKey } from "../../lib/tauri";
import { formatEndpoint } from "./connectionState";

interface ConfirmationSession {
  scope: string;
  running: boolean;
  discardPending: (() => void) | null;
}

function unknownHostChallenge(error: unknown): string | null {
  return typeof error === "object" && error !== null && "code" in error &&
    error.code === "SSH_HOST_KEY_UNKNOWN" && "challenge_id" in error && typeof error.challenge_id === "string"
    ? error.challenge_id : null;
}

/** Keep the scope stable while a connection request is waiting for confirmation. */
export function useSshHostKeyConfirmation(scope: string) {
  const { confirm, confirmationDialog } = useConfirmDialog(scope);
  const activeSession = useRef<ConfirmationSession | null>(null);

  useLayoutEffect(() => {
    const session: ConfirmationSession = { scope, running: false, discardPending: null };
    activeSession.current = session;
    return () => {
      activeSession.current = null;
      session.discardPending?.();
    };
  }, [scope]);

  const runWithSshHostKeyConfirmation = useCallback(async <T,>(
    operation: () => Promise<T>,
    onStaleSuccess?: (result: T) => Promise<void>,
  ): Promise<T | null> => {
    const session = activeSession.current;
    if (!session || session.scope !== scope || session.running) return null;
    session.running = true;
    const isCurrent = () => activeSession.current === session;
    const finish = async (result: T): Promise<T | null> => {
      if (isCurrent()) return result;
      await onStaleSuccess?.(result);
      return null;
    };
    let pendingId: string | null = null;
    const discardPending = () => {
      const id = pendingId;
      pendingId = null;
      // Discard is best effort: records also expire in the backend.
      if (id) void discardSshHostKey(id).catch(() => {});
    };
    session.discardPending = discardPending;

    try {
      try {
        const result = await operation();
        return await finish(result);
      } catch (error) {
        pendingId = unknownHostChallenge(error);
        if (!isCurrent()) return null;
        if (!pendingId) throw error;
      }

      const challenge = await getSshHostKeyChallenge(pendingId);
      if (!isCurrent()) return null;
      if (challenge.challenge_id !== pendingId) {
        throw { code: "IPC_ERROR", message: "SSH 主机指纹确认凭据不匹配" };
      }
      const accepted = await confirm([
        "这是 Redix 首次连接此 SSH 主机，请通过可信渠道核对以下指纹。",
        `SSH 主机：${formatEndpoint(challenge)}`,
        `密钥算法：${challenge.algorithm}`,
        `指纹：${challenge.fingerprint}`,
        "确认后将保存到 Redix 的主机信任库，并继续本次连接。",
      ].join("\n"), { title: "确认 SSH 主机指纹", confirmLabel: "信任并连接", danger: false });
      if (!accepted || !isCurrent()) return null;

      await trustSshHostKey(pendingId);
      pendingId = null;
      if (!isCurrent()) return null;
      // Retry exactly once; a second unknown key must require a new user action.
      try {
        const result = await operation();
        return await finish(result);
      } catch (error) {
        pendingId = unknownHostChallenge(error);
        throw error;
      }
    } catch (error) {
      if (!isCurrent()) return null;
      throw error;
    } finally {
      discardPending();
      session.discardPending = null;
      session.running = false;
    }
  }, [confirm, scope]);

  return { runWithSshHostKeyConfirmation, confirmationDialog };
}
