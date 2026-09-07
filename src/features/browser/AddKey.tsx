import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";

import { createArray, createKey, createVectorSet } from "../../lib/tauri";
import type {
  CreateArrayInput,
  CreateKeyInput,
  CreateVectorSetInput,
  KeyValue,
  RedisValue,
  StreamEntry,
  VectorSetElementPayload,
} from "../../lib/types";
import { browserErrorMessage } from "./browserState";

type CreateValueKind =
  | "string"
  | "hash"
  | "list"
  | "set"
  | "zset"
  | "json"
  | "stream"
  | "array"
  | "vectorset";

interface AddKeyProps {
  connectionId: string;
  busy: boolean;
  arraySupported?: boolean;
  vectorSetSupported?: boolean;
  onCreated: (detail: KeyValue) => void;
  onCancel: () => void;
}

function parseArrayCreate(
  mode: "contiguous" | "sparse",
  startIndex: string,
  valuesText: string,
): Pick<CreateArrayInput, "mode" | "start_index" | "values" | "elements" | "ttl_ms"> {
  const lines = parseNonEmptyLines(valuesText);
  if (lines.length === 0) {
    throw new Error("Array 至少需要一个值或稀疏元素。");
  }
  if (mode === "contiguous") {
    if (startIndex.trim() !== "" && !/^\d{1,20}$/.test(startIndex.trim())) {
      throw new Error("Array 起始索引必须是 0 到 20 位的非负整数。");
    }
    return {
      mode,
      start_index: startIndex.trim() || null,
      values: lines,
      elements: [],
      ttl_ms: null,
    };
  }

  const elements = lines.map((line) => {
    const separator = line.indexOf("=");
    const index = line.slice(0, separator).trim();
    if (separator <= 0 || !/^\d{1,20}$/.test(index)) {
      throw new Error("稀疏 Array 每行必须使用 index=value 格式。");
    }
    return { index, value: line.slice(separator + 1) };
  });
  if (new Set(elements.map((element) => element.index)).size !== elements.length) {
    throw new Error("Array 索引不能重复。");
  }
  return {
    mode,
    start_index: null,
    values: [],
    elements,
    ttl_ms: null,
  };
}

function parseVectorElements(text: string): VectorSetElementPayload[] {
  const lines = parseNonEmptyLines(text);
  if (lines.length === 0) {
    throw new Error("Vector Set 至少需要一个元素。");
  }
  const names = new Set<string>();
  return lines.map((line) => {
    const separator = line.indexOf("|");
    const name = line.slice(0, separator).trim();
    if (separator <= 0 || name === "" || names.has(name)) {
      throw new Error("Vector Set 每行必须使用 name|[向量] 格式，名称不能重复。");
    }
    names.add(name);
    let values: unknown;
    try {
      values = JSON.parse(line.slice(separator + 1));
    } catch {
      throw new Error("Vector Set 向量必须是 JSON 数组。");
    }
    if (
      !Array.isArray(values) ||
      values.length === 0 ||
      values.some((value) => typeof value !== "number" || !Number.isFinite(value))
    ) {
      throw new Error("Vector Set 向量必须是只包含有限数字的 JSON 数组。");
    }
    return {
      name,
      vector_values: values,
      vector_fp32_base64: null,
      attributes: null,
    };
  });
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

export function AddKey({
  connectionId,
  busy,
  arraySupported = false,
  vectorSetSupported = false,
  onCreated,
  onCancel,
}: AddKeyProps) {
  const [key, setKey] = useState("");
  const [kind, setKind] = useState<CreateValueKind>("string");
  const [stringValue, setStringValue] = useState("");
  const [collectionText, setCollectionText] = useState("");
  const [jsonText, setJsonText] = useState('{"value":""}');
  const [streamText, setStreamText] = useState(
    '[{"id":"*","fields":[{"field":"event","value":""}]}]',
  );
  const [arrayMode, setArrayMode] = useState<"contiguous" | "sparse">("contiguous");
  const [arrayStartIndex, setArrayStartIndex] = useState("0");
  const [vectorDimension, setVectorDimension] = useState("");
  const [vectorQuantization, setVectorQuantization] = useState("");
  const [vectorText, setVectorText] = useState("");
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

    let value: RedisValue | null = null;
    let arrayInput: Pick<
      CreateArrayInput,
      "mode" | "start_index" | "values" | "elements" | "ttl_ms"
    > | null = null;
    let vectorInput: VectorSetElementPayload[] | null = null;
    try {
      if (kind === "array") {
        arrayInput = parseArrayCreate(arrayMode, arrayStartIndex, collectionText);
      } else if (kind === "vectorset") {
        vectorInput = parseVectorElements(vectorText);
        const dimension = Number(vectorDimension);
        if (!Number.isInteger(dimension) || dimension < 1) {
          throw new Error("Vector Set 维度必须是正整数。");
        }
        if (vectorInput.some((element) => element.vector_values?.length !== dimension)) {
          throw new Error("每个 Vector Set 元素的维度必须与设置值一致。");
        }
      } else {
        value = parseValue(kind, stringValue, collectionText, jsonText, streamText);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "数据格式无效。");
      return;
    }

    const requestConnectionId = connectionId;
    setSubmitting(true);
    setError(null);
    try {
      let detail: KeyValue;
      if (kind === "array" && arrayInput) {
        detail = await createArray({
          connection_id: requestConnectionId,
          key: normalizedKey,
          ...arrayInput,
          ttl_ms: ttlMs,
        } satisfies CreateArrayInput);
      } else if (kind === "vectorset" && vectorInput) {
        detail = await createVectorSet({
          connection_id: requestConnectionId,
          key: normalizedKey,
          dimension: Number(vectorDimension),
          quantization: vectorQuantization.trim() || null,
          elements: vectorInput,
          ttl_ms: ttlMs,
        } satisfies CreateVectorSetInput);
      } else if (value) {
        detail = await createKey({
          connection_id: requestConnectionId,
          key: normalizedKey,
          value,
          ttl_ms: ttlMs,
        } satisfies CreateKeyInput);
      } else {
        throw new Error("创建数据格式无效。");
      }
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
          <Select value={kind} onChange={(event) => setKind(event.target.value as CreateValueKind)} disabled={isBusy}>
            <option value="string">String</option>
            <option value="hash">Hash</option>
            <option value="list">List</option>
            <option value="set">Set</option>
            <option value="zset">Sorted Set</option>
            <option value="json">JSON</option>
            <option value="stream">Stream</option>
            {arraySupported ? <option value="array">Array</option> : null}
            {vectorSetSupported ? <option value="vectorset">Vector Set</option> : null}
          </Select>
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
      {kind === "array" ? (
        <>
          <label className="field">
            <span>Array 创建模式</span>
            <Select value={arrayMode} onChange={(event) => setArrayMode(event.target.value as "contiguous" | "sparse")} disabled={isBusy}>
              <option value="contiguous">连续索引</option>
              <option value="sparse">稀疏索引</option>
            </Select>
          </label>
          {arrayMode === "contiguous" ? (
            <label className="field">
              <span>Array 起始索引</span>
              <input aria-label="Array 起始索引" value={arrayStartIndex} onChange={(event) => setArrayStartIndex(event.target.value)} disabled={isBusy} inputMode="numeric" />
            </label>
          ) : null}
          <label className="field">
            <span>Array 值</span>
            <textarea aria-label="Array 值" value={collectionText} onChange={(event) => setCollectionText(event.target.value)} disabled={isBusy} placeholder={arrayMode === "sparse" ? "index=value" : "每行一个值"} spellCheck={false} />
          </label>
        </>
      ) : null}
      {kind === "vectorset" ? (
        <>
          <div className="browser-add-key-grid">
            <label className="field"><span>Vector Set 维度</span><input aria-label="Vector Set 维度" type="number" min="1" step="1" value={vectorDimension} onChange={(event) => setVectorDimension(event.target.value)} disabled={isBusy} /></label>
            <label className="field"><span>量化类型（可选）</span><input value={vectorQuantization} onChange={(event) => setVectorQuantization(event.target.value)} disabled={isBusy} placeholder="f32" /></label>
          </div>
          <label className="field"><span>Vector Set 元素</span><textarea aria-label="Vector Set 元素" value={vectorText} onChange={(event) => setVectorText(event.target.value)} disabled={isBusy} placeholder={'每行 name|[0.1,0.2]'} spellCheck={false} /></label>
        </>
      ) : null}
      {kind !== "string" && kind !== "json" && kind !== "stream" && kind !== "array" && kind !== "vectorset" ? (
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
