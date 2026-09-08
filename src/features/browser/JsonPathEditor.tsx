import { useEffect, useLayoutEffect, useRef, useState } from "react";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useConfirmDialog } from "../../components/useConfirmDialog";

import type {
  JsonMutationResult,
  JsonPathValue,
  JsonValue,
} from "../../lib/types";
import { formatJsonValue, formatTtl } from "./browserState";
import { JsonTree } from "./JsonTree";

export type JsonPathMutation =
  | { kind: "set"; path: string; value: JsonValue }
  | { kind: "append"; path: string; values: JsonValue[] }
  | { kind: "delete"; path: string };

interface JsonPathEditorProps {
  value: JsonValue;
  busy: boolean;
  error: string | null;
  errorResetKey?: unknown;
  rootDeleteMessage?: string;
  onRead(path: string): Promise<JsonPathValue>;
  onMutate(mutation: JsonPathMutation): Promise<JsonMutationResult>;
}

function parseJsonDraft(draft: string): JsonValue {
  return JSON.parse(draft) as JsonValue;
}

function sameJsonValue(left: JsonValue, right: JsonValue): boolean {
  const pending: Array<[JsonValue, JsonValue]> = [[left, right]];
  while (pending.length > 0) {
    const [a, b] = pending.pop()!;
    if (a === b) continue;
    if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
    if (Array.isArray(a) || Array.isArray(b)) {
      if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
      for (let index = 0; index < a.length; index += 1) pending.push([a[index], b[index]]);
    } else {
      const keys = Object.keys(a);
      if (keys.length !== Object.keys(b).length) return false;
      for (const key of keys) {
        if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
        pending.push([a[key], b[key]]);
      }
    }
  }
  return true;
}

export function JsonPathEditor({
  value,
  busy,
  error,
  errorResetKey,
  rootDeleteMessage,
  onRead,
  onMutate,
}: JsonPathEditorProps) {
  const [path, setPath] = useState("$");
  const [jsonDraft, setJsonDraft] = useState(() => formatJsonValue(value));
  const [validationError, setValidationError, validationErrorToken] = useFeedbackState<string | null>(null);
  const [operationError, setOperationError, operationErrorToken] = useFeedbackState<string | null>(null);
  const [readResult, setReadResult] = useState<JsonPathValue | null>(null);
  const [blockedReason, setBlockedReason] = useState<string | null>(null);
  const { confirm, confirmationDialog } = useConfirmDialog(
    JSON.stringify([path, jsonDraft, busy, rootDeleteMessage]),
  );
  const currentConfirmRef = useRef(confirm);
  currentConfirmRef.current = confirm;
  const deletingRef = useRef(false);
  const mountedRef = useRef(false);
  const readRequestRef = useRef(0);
  const sourceValueRef = useRef(value);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      readRequestRef.current += 1;
    };
  }, []);

  useLayoutEffect(() => {
    // Mount is already initialized by useState. Metadata refreshes can also
    // provide a new, equivalent JSON object without invalidating the draft.
    const unchanged = sameJsonValue(sourceValueRef.current, value);
    sourceValueRef.current = value;
    if (unchanged) return;
    readRequestRef.current += 1;
    setPath("$");
    setBlockedReason(null);
    setJsonDraft(formatJsonValue(value));
    setValidationError(null);
    setOperationError(null);
    setReadResult(null);
  }, [value]);

  const validatePath = () => {
    if (blockedReason) return null;
    const trimmedPath = path.trim();
    if (trimmedPath === "") {
      setValidationError("JSON Path 不能为空。");
      setOperationError(null);
      return null;
    }
    return trimmedPath;
  };

  const handleRead = async () => {
    const trimmedPath = validatePath();
    if (trimmedPath === null) {
      return;
    }

    const requestId = readRequestRef.current + 1;
    readRequestRef.current = requestId;
    setValidationError(null);
    setOperationError(null);
    setReadResult(null);
    try {
      const result = await onRead(trimmedPath);
      if (mountedRef.current && readRequestRef.current === requestId) {
        if (!result.found) {
          setOperationError("未找到匹配的 JSON Path。");
          setReadResult(null);
          return;
        }
        setReadResult(result);
      }
    } catch {
      if (mountedRef.current && readRequestRef.current === requestId && !error) {
        setOperationError("读取 JSON Path 失败，请稍后重试。");
      }
    }
  };

  const mutate = async (kind: JsonPathMutation["kind"]) => {
    if (busy || deletingRef.current) return;
    const trimmedPath = validatePath();
    if (trimmedPath === null) {
      return;
    }

    if (kind === "delete") {
      setValidationError(null);
      setOperationError(null);
      if (rootDeleteMessage && (trimmedPath === "$" || trimmedPath === ".")) {
        const requestId = readRequestRef.current;
        if (!await confirm(rootDeleteMessage)) return;
        if (!mountedRef.current || currentConfirmRef.current !== confirm
          || readRequestRef.current !== requestId || deletingRef.current) return;
      }
      deletingRef.current = true;
      try {
        await onMutate({ kind, path: trimmedPath });
      } catch {
        if (mountedRef.current && !error) {
          setOperationError("JSON Path 操作失败，请稍后重试。");
        }
      } finally {
        deletingRef.current = false;
      }
      return;
    }

    if (jsonDraft.trim() === "") {
      setValidationError("JSON 值不能为空。");
      setOperationError(null);
      return;
    }

    let parsed: JsonValue;
    try {
      parsed = parseJsonDraft(jsonDraft);
    } catch {
      setValidationError("JSON 格式无效。");
      setOperationError(null);
      return;
    }
    if (kind === "append") {
      if (!Array.isArray(parsed)) {
        setValidationError("数组追加需要 JSON 数组。");
        setOperationError(null);
        return;
      }
      setValidationError(null);
      setOperationError(null);
      try {
        await onMutate({ kind, path: trimmedPath, values: parsed });
      } catch {
        if (!error) {
          setOperationError("JSON Path 操作失败，请稍后重试。");
        }
      }
      return;
    }

    setValidationError(null);
    setOperationError(null);
    try {
      await onMutate({ kind, path: trimmedPath, value: parsed });
    } catch {
      if (!error) {
        setOperationError("JSON Path 操作失败，请稍后重试。");
      }
    }
  };

  const visibleError = error ?? validationError ?? operationError;
  const visibleErrorToken = error
    ? errorResetKey ?? error
    : validationError
      ? validationErrorToken
      : operationErrorToken;

  return (
    <section className="json-path-editor" aria-labelledby="json-path-editor-title" aria-busy={busy}>
      {confirmationDialog}
      <div className="editor-heading">
        <div>
          <p className="eyebrow">JSON PATH</p>
          <h3 id="json-path-editor-title">路径编辑器</h3>
        </div>
        <span className="editor-type">json</span>
      </div>

      <JsonTree value={value} selectedPath={path} busy={busy} onSelect={(node) => {
        readRequestRef.current += 1;
        setPath(node.path);
        setJsonDraft(formatJsonValue(node.value));
        setBlockedReason(node.blockedReason);
        setValidationError(null);
        setOperationError(null);
        setReadResult(null);
      }} />

      {blockedReason ? <p className="browser-helper" role="status">{blockedReason}</p> : null}

      <label className="field">
        <span>JSON Path</span>
        <input
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="JSON Path"
          value={path}
          onChange={(event) => {
            readRequestRef.current += 1;
            setPath(event.target.value);
            setBlockedReason(null);
            setValidationError(null);
            setOperationError(null);
            setReadResult(null);
          }}
          disabled={busy}
          spellCheck={false}
        />
      </label>

      <label className="field">
        <span>路径 JSON 值</span>
        <textarea
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="路径 JSON 值"
          value={jsonDraft}
          onChange={(event) => {
            setJsonDraft(event.target.value);
            setValidationError(null);
            setOperationError(null);
          }}
          disabled={busy}
          spellCheck={false}
        />
      </label>

      {visibleError ? <Toast
        kind="error"
        message={visibleError}
        onClose={error ? undefined : () => { setValidationError(null); setOperationError(null); }}
        resetKey={visibleErrorToken}
      /> : null}

      {readResult ? (
        <div className="json-path-result">
          <div className="json-path-result-meta">
            <span>路径：{readResult.path}</span>
            <span>TTL：{formatTtl(readResult.ttl_ms)}</span>
          </div>
          <pre aria-label="路径读取结果">{formatJsonValue(readResult.value)}</pre>
        </div>
      ) : null}

      <div className="editor-actions json-path-actions">
        <button
          type="button"
          className="button button-secondary"
          onClick={() => void handleRead()}
          disabled={busy || blockedReason !== null}
        >
          读取路径
        </button>
        <button
          type="button"
          className="button button-primary"
          onClick={() => void mutate("set")}
          disabled={busy || blockedReason !== null}
        >
          保存路径
        </button>
        <button
          type="button"
          className="button button-secondary"
          onClick={() => void mutate("append")}
          disabled={busy || blockedReason !== null}
        >
          数组追加
        </button>
        <button
          type="button"
          className="button button-danger"
          onClick={() => void mutate("delete")}
          disabled={busy || blockedReason !== null}
        >
          删除路径
        </button>
      </div>
    </section>
  );
}

export default JsonPathEditor;
