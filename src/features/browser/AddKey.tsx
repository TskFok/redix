import { useEffect, useRef, useState } from "react";

import { createKey } from "../../lib/tauri";
import type {
  CreateKeyInput,
  KeyValue,
  RedisValue,
  StreamEntry,
} from "../../lib/types";
import { browserErrorMessage } from "./browserState";

type CreateValueKind = "string" | "hash" | "list" | "set" | "zset" | "json" | "stream";

interface AddKeyProps {
  connectionId: string;
  busy: boolean;
  onCreated: (detail: KeyValue) => void;
  onCancel: () => void;
}

function parseNonEmptyLines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function parseValue(
  kind: CreateValueKind,
  stringValue: string,
  collectionText: string,
  jsonText: string,
  streamText: string,
): RedisValue {
  if (kind === "string") {
    return { String: { value: stringValue } };
  }
  if (kind === "json") {
    return { Json: { value: JSON.parse(jsonText) } };
  }
  if (kind === "stream") {
    const entries = JSON.parse(streamText) as StreamEntry[];
    if (
      !Array.isArray(entries) ||
      entries.length === 0 ||
      entries.some(
        (entry) =>
          typeof entry?.id !== "string" ||
          !Array.isArray(entry.fields) ||
          entry.fields.length === 0 ||
          entry.fields.some(
            (field) =>
              typeof field?.field !== "string" ||
              field.field.trim() === "" ||
              typeof field.value !== "string",
          ),
      )
    ) {
      throw new Error("Stream 至少需要一条包含字段和值的记录。");
    }
    return { Stream: { entries } };
  }

  const lines = parseNonEmptyLines(collectionText);
  if (lines.length === 0) {
    throw new Error("至少填写一项数据。");
  }
  if (kind === "list") {
    return { List: { items: lines } };
  }
  if (kind === "set") {
    return { Set: { members: [...new Set(lines)] } };
  }
  if (kind === "hash") {
    const fields = lines.map((line) => {
      const separator = line.indexOf("=");
      if (separator <= 0) {
        throw new Error("Hash 每行必须使用 field=value 格式。");
      }
      return { field: line.slice(0, separator).trim(), value: line.slice(separator + 1) };
    });
    if (
      fields.some((field) => field.field === "") ||
      new Set(fields.map((field) => field.field)).size !== fields.length
    ) {
      throw new Error("Hash 字段名不能为空且不能重复。");
    }
    return { Hash: { fields } };
  }

  const members = lines.map((line) => {
    const separator = line.lastIndexOf("=");
    const member = line.slice(0, separator).trim();
    const score = Number(line.slice(separator + 1));
    if (separator <= 0 || member === "" || !Number.isFinite(score)) {
      throw new Error("Sorted Set 每行必须使用 member=数字格式。");
    }
    return { member, score };
  });
  return { SortedSet: { members } };
}

export function AddKey({ connectionId, busy, onCreated, onCancel }: AddKeyProps) {
  const [key, setKey] = useState("");
  const [kind, setKind] = useState<CreateValueKind>("string");
  const [stringValue, setStringValue] = useState("");
  const [collectionText, setCollectionText] = useState("");
  const [jsonText, setJsonText] = useState('{"value":""}');
  const [streamText, setStreamText] = useState(
    '[{"id":"*","fields":[{"field":"event","value":""}]}]',
  );
  const [ttlText, setTtlText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const mountedRef = useRef(false);
  const connectionRef = useRef(connectionId);
  connectionRef.current = connectionId;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedKey = key.trim();
    if (normalizedKey === "") {
      setError("键名不能为空。");
      return;
    }

    const ttlTextValue = ttlText.trim();
    const ttlMs = ttlTextValue === "" ? null : Number(ttlTextValue);
    if (ttlMs !== null && (!Number.isInteger(ttlMs) || ttlMs < 0)) {
      setError("TTL 必须是大于等于 0 的整数毫秒。");
      return;
    }

    let value: RedisValue;
    try {
      value = parseValue(kind, stringValue, collectionText, jsonText, streamText);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "数据格式无效。");
      return;
    }

    const requestConnectionId = connectionId;
    setSubmitting(true);
    setError(null);
    try {
      const detail = await createKey({
        connection_id: requestConnectionId,
        key: normalizedKey,
        value,
        ttl_ms: ttlMs,
      } satisfies CreateKeyInput);
      if (
        mountedRef.current &&
        connectionRef.current === requestConnectionId
      ) {
        onCreated(detail);
      }
    } catch (caught) {
      if (mountedRef.current && connectionRef.current === requestConnectionId) {
        setError(browserErrorMessage(caught, "创建键失败，请稍后重试。"));
      }
    } finally {
      if (mountedRef.current && connectionRef.current === requestConnectionId) {
        setSubmitting(false);
      }
    }
  };

  const isBusy = busy || submitting;

  return (
    <form className="browser-add-key" onSubmit={(event) => void handleSubmit(event)} aria-busy={isBusy}>
      <div className="browser-add-key-heading">
        <div>
          <p className="eyebrow">CREATE</p>
          <h3>新增键</h3>
        </div>
        <button type="button" className="button button-quiet" onClick={onCancel} disabled={isBusy}>
          取消
        </button>
      </div>
      <div className="browser-add-key-grid">
        <label className="field">
          <span>键名</span>
          <input
            value={key}
            onChange={(event) => setKey(event.target.value)}
            disabled={isBusy}
            autoFocus
            spellCheck={false}
          />
        </label>
        <label className="field">
          <span>数据类型</span>
          <select value={kind} onChange={(event) => setKind(event.target.value as CreateValueKind)} disabled={isBusy}>
            <option value="string">String</option>
            <option value="hash">Hash</option>
            <option value="list">List</option>
            <option value="set">Set</option>
            <option value="zset">Sorted Set</option>
            <option value="json">JSON</option>
            <option value="stream">Stream</option>
          </select>
        </label>
      </div>

      {kind === "string" ? (
        <label className="field">
          <span>字符串值</span>
          <textarea value={stringValue} onChange={(event) => setStringValue(event.target.value)} disabled={isBusy} />
        </label>
      ) : null}
      {kind === "json" ? (
        <label className="field">
          <span>JSON 文档</span>
          <textarea value={jsonText} onChange={(event) => setJsonText(event.target.value)} disabled={isBusy} spellCheck={false} />
        </label>
      ) : null}
      {kind === "stream" ? (
        <label className="field">
          <span>Stream 条目 JSON</span>
          <textarea value={streamText} onChange={(event) => setStreamText(event.target.value)} disabled={isBusy} spellCheck={false} />
        </label>
      ) : null}
      {kind !== "string" && kind !== "json" && kind !== "stream" ? (
        <label className="field">
          <span>{kind === "hash" || kind === "zset" ? "集合值（每行一项）" : "集合值（每行一个）"}</span>
          <textarea
            value={collectionText}
            onChange={(event) => setCollectionText(event.target.value)}
            disabled={isBusy}
            placeholder={kind === "hash" ? "field=value" : kind === "zset" ? "member=1" : "item"}
            spellCheck={false}
          />
        </label>
      ) : null}
      <div className="browser-add-key-footer">
        <label className="field">
          <span>TTL（毫秒，可选）</span>
          <input type="number" min="0" step="1" value={ttlText} onChange={(event) => setTtlText(event.target.value)} disabled={isBusy} />
        </label>
        <button type="submit" className="button button-primary" disabled={isBusy}>
          {submitting ? "创建中…" : "创建键"}
        </button>
      </div>
      {error ? (
        <p className="feedback feedback-error" role="alert">
          {error}
        </p>
      ) : null}
    </form>
  );
}

export default AddKey;
