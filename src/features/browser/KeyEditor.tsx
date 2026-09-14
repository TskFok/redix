import { useState } from "react";
import Select from "../../components/Select";
import { byteIdentity, bytesFromInput, bytesToInput, bytesToSingleLineInput, bytesToMultilineInput, isBinary, type ByteFormat, type RedisBytes } from "../../lib/redisBytes";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";

import type { RedisValue } from "../../lib/types";
import {
  cloneRedisValue,
  redisValueKind,
  type RedisValueKind,
} from "./browserState";

interface KeyEditorProps {
  value: RedisValue;
  showKeyActions?: boolean;
  ttlMs: number;
  busy: boolean;
  error: string | null;
  onSave: (value: RedisValue) => Promise<void>;
  onDelete: () => Promise<void>;
  onSetTtl: (ttlMs: number) => Promise<void>;
}

function mapValueBytes(value: RedisValue, convert: (value: RedisBytes, path: string) => RedisBytes): RedisValue {
  if ("String" in value) return { String: { value: convert(value.String.value, "string") } };
  if ("Hash" in value) return { Hash: { fields: value.Hash.fields.map((entry, index) => ({ field: convert(entry.field, `hash.${index}.field`), value: convert(entry.value, `hash.${index}.value`) })) } };
  if ("List" in value) return { List: { items: value.List.items.map((entry, index) => convert(entry, `list.${index}`)) } };
  if ("Set" in value) return { Set: { members: value.Set.members.map((entry, index) => convert(entry, `set.${index}`)) } };
  if ("SortedSet" in value) return { SortedSet: { members: value.SortedSet.members.map((entry, index) => ({ ...entry, member: convert(entry.member, `sorted-set.${index}`) })) } };
  return cloneRedisValue(value);
}

function prepareEditor(value: RedisValue) {
  const formats: Record<string, ByteFormat> = {};
  const draft = mapValueBytes(value, (bytes, path) => {
    const format = isBinary(bytes) ? "hex" : "utf8";
    formats[path] = format;
    return bytesToInput(bytes, format);
  });
  return { draft, formats };
}

// Basic byte fields in the editor draft contain the text of their selected encoding.
function draftText(value: RedisBytes): string {
  return typeof value === "string" ? value : bytesToInput(value, "hex");
}

function formatJson(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

function editorTitle(kind: RedisValueKind): string {
  switch (kind) {
    case "string":
      return "String 值";
    case "hash":
      return "Hash 字段";
    case "list":
      return "List 元素";
    case "set":
      return "Set 成员";
    case "sorted-set":
      return "Sorted Set 成员";
    case "json":
      return "JSON 文档";
    case "stream":
      return "Stream 条目";
    case "array":
      return "Array 数据";
    case "vectorset":
      return "Vector Set 数据";
  }
}

export function KeyEditor({
  value,
  showKeyActions = true,
  ttlMs,
  busy,
  error,
  onSave,
  onDelete,
  onSetTtl,
}: KeyEditorProps) {
  const [initial] = useState(() => prepareEditor(value));
  const [draft, setDraft] = useState<RedisValue>(initial.draft);
  const [formats, setFormats] = useState(initial.formats);
  const [jsonDraft, setJsonDraft] = useState(() =>
    "Json" in value ? formatJson(value.Json.value) : "",
  );
  const [ttlDraft, setTtlDraft] = useState(() => (ttlMs >= 0 ? String(ttlMs) : ""));
  const [validationError, setValidationError, validationErrorToken] = useFeedbackState<string | null>(null);

  const kind = redisValueKind(draft);

  const encodingSelect = (path: string, label: string, text: RedisBytes, update: (text: string) => void, multiline = false) => <label className="field">
    <span>{label}编码</span>
    <Select aria-label={`${label}编码`} value={formats[path] ?? "utf8"} disabled={busy} onChange={(event) => {
      const nextFormat = event.target.value as ByteFormat;
      try {
        const encoded = (multiline ? bytesToMultilineInput : bytesToSingleLineInput)(bytesFromInput(draftText(text), formats[path] ?? "utf8"), nextFormat);
        update(encoded);
        setFormats((previous) => ({ ...previous, [path]: nextFormat }));
        setValidationError(null);
      } catch (caught) { setValidationError(caught instanceof Error ? caught.message : "数据编码无效。"); }
    }}><option value="utf8">UTF-8</option><option value="hex">Hex</option><option value="base64">Base64</option></Select>
  </label>;

  const updateString = (next: string) => {
    setDraft({ String: { value: next } });
    setValidationError(null);
  };

  const updateHashField = (index: number, field: "field" | "value", next: string) => {
    if (!("Hash" in draft)) {
      return;
    }
    const fields = draft.Hash.fields.map((entry, entryIndex) =>
      entryIndex === index ? { ...entry, [field]: next } : entry,
    );
    setDraft({ Hash: { fields } });
    setValidationError(null);
  };

  const updateListItem = (index: number, next: string) => {
    if (!("List" in draft)) {
      return;
    }
    const items = draft.List.items.map((item, itemIndex) =>
      itemIndex === index ? next : item,
    );
    setDraft({ List: { items } });
    setValidationError(null);
  };

  const updateSetMember = (index: number, next: string) => {
    if (!("Set" in draft)) {
      return;
    }
    const members = draft.Set.members.map((member, memberIndex) =>
      memberIndex === index ? next : member,
    );
    setDraft({ Set: { members } });
    setValidationError(null);
  };

  const updateSortedSetMember = (
    index: number,
    field: "member" | "score",
    next: string,
  ) => {
    if (!("SortedSet" in draft)) {
      return;
    }
    const members = draft.SortedSet.members.map((entry, entryIndex) => {
      if (entryIndex !== index) {
        return entry;
      }
      if (field === "member") {
        return { ...entry, member: next };
      }
      return { ...entry, score: next.trim() === "" ? Number.NaN : Number(next) };
    });
    setDraft({ SortedSet: { members } });
    setValidationError(null);
  };

  const updateStreamEntry = (
    entryIndex: number,
    fieldIndex: number,
    field: "field" | "value",
    next: string,
  ) => {
    if (!("Stream" in draft)) {
      return;
    }
    const entries = draft.Stream.entries.map((entry, currentEntryIndex) => {
      if (currentEntryIndex !== entryIndex) {
        return entry;
      }
      return {
        ...entry,
        fields: entry.fields.map((item, currentFieldIndex) =>
          currentFieldIndex === fieldIndex ? { ...item, [field]: next } : item,
        ),
      };
    });
    setDraft({ Stream: { entries } });
    setValidationError(null);
  };

  const updateStreamEntryId = (entryIndex: number, next: string) => {
    if (!("Stream" in draft)) {
      return;
    }
    setDraft({
      Stream: {
        entries: draft.Stream.entries.map((entry, currentEntryIndex) =>
          currentEntryIndex === entryIndex ? { ...entry, id: next } : entry,
        ),
      },
    });
    setValidationError(null);
  };

  const addRow = () => {
    if ("Hash" in draft) {
      setDraft({
        Hash: { fields: [...draft.Hash.fields, { field: "", value: "" }] },
      });
    } else if ("List" in draft) {
      setDraft({ List: { items: [...draft.List.items, ""] } });
    } else if ("Set" in draft) {
      setDraft({ Set: { members: [...draft.Set.members, ""] } });
    } else if ("SortedSet" in draft) {
      setDraft({
        SortedSet: {
          members: [...draft.SortedSet.members, { member: "", score: 0 }],
        },
      });
    } else if ("Stream" in draft) {
      setDraft({
        Stream: {
          entries: [
            ...draft.Stream.entries,
            { id: "*", fields: [{ field: "", value: "" }] },
          ],
        },
      });
    }
    setValidationError(null);
  };

  const removeRow = (index: number) => {
    setFormats((previous) => Object.fromEntries(Object.entries(previous).flatMap(([path, format]) => {
      const [prefix, row, ...tail] = path.split(".");
      if (prefix !== kind || row === undefined || Number(row) < index) return [[path, format]];
      if (Number(row) === index) return [];
      return [[[prefix, String(Number(row) - 1), ...tail].join("."), format]];
    })));

    if ("Hash" in draft) {
      setDraft({ Hash: { fields: draft.Hash.fields.filter((_, row) => row !== index) } });
    } else if ("List" in draft) {
      setDraft({ List: { items: draft.List.items.filter((_, row) => row !== index) } });
    } else if ("Set" in draft) {
      setDraft({ Set: { members: draft.Set.members.filter((_, row) => row !== index) } });
    } else if ("SortedSet" in draft) {
      setDraft({
        SortedSet: {
          members: draft.SortedSet.members.filter((_, row) => row !== index),
        },
      });
    } else if ("Stream" in draft) {
      setDraft({
        Stream: {
          entries: draft.Stream.entries.filter((_, row) => row !== index),
        },
      });
    }
    setValidationError(null);
  };

  const handleSave = async () => {
    let nextValue: RedisValue;
    try {
      nextValue = mapValueBytes(draft, (text, path) => bytesFromInput(draftText(text), formats[path] ?? "utf8"));
    } catch (caught) {
      setValidationError(caught instanceof Error ? caught.message : "数据编码无效。");
      return;
    }
    if ("Json" in nextValue) {
      try {
        nextValue = { Json: { value: JSON.parse(jsonDraft) } };
      } catch {
        setValidationError("JSON 格式无效。");
        return;
      }
    }
    if ("Set" in nextValue) {
      nextValue = {
        Set: { members: [...new Map(nextValue.Set.members.map((member) => [byteIdentity(member), member])).values()] },
      };
    }
    if (
      ("Hash" in nextValue && nextValue.Hash.fields.length === 0) ||
      ("List" in nextValue && nextValue.List.items.length === 0) ||
      ("Set" in nextValue && nextValue.Set.members.length === 0) ||
      ("SortedSet" in nextValue && nextValue.SortedSet.members.length === 0) ||
      ("Stream" in nextValue && nextValue.Stream.entries.length === 0)
    ) {
      setValidationError("至少保留一项，或使用“删除”操作删除键。");
      return;
    }
    if (
      "SortedSet" in nextValue &&
      nextValue.SortedSet.members.some((entry) => !Number.isFinite(entry.score))
    ) {
      setValidationError("分数必须是有限数字。");
      return;
    }
    if ("Stream" in nextValue) {
      const invalidStream = nextValue.Stream.entries.some((entry) => {
        const names = new Set(entry.fields.map((field) => field.field.trim()));
        return (
          entry.id.trim() === "" ||
          entry.fields.length === 0 ||
          entry.fields.some((field) => field.field.trim() === "") ||
          names.size !== entry.fields.length
        );
      });
      if (invalidStream) {
        setValidationError("Stream 字段名不能为空且不能重复。");
        return;
      }
    }

    setValidationError(null);
    await onSave(nextValue);
  };

  const handleTtl = async () => {
    const parsed = ttlDraft.trim() === "" ? Number.NaN : Number(ttlDraft);
    if (!Number.isInteger(parsed) || parsed < 0) {
      setValidationError("TTL 必须是大于等于 0 的整数毫秒。");
      return;
    }

    setValidationError(null);
    await onSetTtl(parsed);
  };

  return (
    <form
      className="key-editor"
      onSubmit={(event) => {
        event.preventDefault();
        void handleSave();
      }}
      aria-busy={busy}
    >
      <div className="editor-heading">
        <div>
          <p className="eyebrow">编辑器</p>
          <h3>{editorTitle(kind)}</h3>
        </div>
        <span className="editor-type">{kind === "sorted-set" ? "zset" : kind}</span>
      </div>

      {"String" in draft ? (
        <><label className="field">
          <span>字符串值</span>
          <textarea
            autoCapitalize="off"
            autoCorrect="off"
            value={draftText(draft.String.value)}
            onChange={(event) => updateString(event.target.value)}
            disabled={busy}
            spellCheck={false}
          />
        </label>
        {encodingSelect("string", "字符串值", draft.String.value, updateString, true)}</>
      ) : null}

      {"Hash" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Hash 字段</legend>
          <div className="editor-rows">
            {draft.Hash.fields.map((entry, index) => (
              <div className="editor-row editor-row-hash" key={`hash-${index}`}>
                <div className="module-tab-content">
                <label className="field">
                  <span>字段 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`字段 ${index + 1}`}
                    value={draftText(entry.field)}
                    onChange={(event) => updateHashField(index, "field", event.target.value)}
                    disabled={busy}
                  />
                </label>
                {encodingSelect(`hash.${index}.field`, `字段 ${index + 1}`, entry.field, (next) => updateHashField(index, "field", next))}
                </div>
                <div className="module-tab-content">
                <label className="field">
                  <span>值 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`值 ${index + 1}`}
                    value={draftText(entry.value)}
                    onChange={(event) => updateHashField(index, "value", event.target.value)}
                    disabled={busy}
                  />
                </label>
                {encodingSelect(`hash.${index}.value`, `值 ${index + 1}`, entry.value, (next) => updateHashField(index, "value", next))}
                </div>
                <button
                  type="button"
                  className="button button-quiet row-remove"
                  onClick={() => removeRow(index)}
                  disabled={busy}
                  aria-label={`删除字段 ${index + 1}`}
                >
                  移除
                </button>
              </div>
            ))}
          </div>
          <button type="button" className="button button-secondary" onClick={addRow} disabled={busy}>
            添加字段
          </button>
        </fieldset>
      ) : null}

      {"List" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>List 元素</legend>
          <div className="editor-rows">
            {draft.List.items.map((item, index) => (
              <div className="editor-row editor-row-single" key={`list-${index}`}>
                <div className="module-tab-content">
                <label className="field">
                  <span>元素 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`元素 ${index + 1}`}
                    value={draftText(item)}
                    onChange={(event) => updateListItem(index, event.target.value)}
                    disabled={busy}
                  />
                </label>
                {encodingSelect(`list.${index}`, `元素 ${index + 1}`, item, (next) => updateListItem(index, next))}
                </div>
                <button
                  type="button"
                  className="button button-quiet row-remove"
                  onClick={() => removeRow(index)}
                  disabled={busy}
                  aria-label={`删除元素 ${index + 1}`}
                >
                  移除
                </button>
              </div>
            ))}
          </div>
          <button type="button" className="button button-secondary" onClick={addRow} disabled={busy}>
            添加元素
          </button>
        </fieldset>
      ) : null}

      {"Set" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Set 成员</legend>
          <div className="editor-rows">
            {draft.Set.members.map((member, index) => (
              <div className="editor-row editor-row-single" key={`set-${index}`}>
                <div className="module-tab-content">
                <label className="field">
                  <span>成员 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`成员 ${index + 1}`}
                    value={draftText(member)}
                    onChange={(event) => updateSetMember(index, event.target.value)}
                    disabled={busy}
                  />
                </label>
                {encodingSelect(`set.${index}`, `成员 ${index + 1}`, member, (next) => updateSetMember(index, next))}
                </div>
                <button
                  type="button"
                  className="button button-quiet row-remove"
                  onClick={() => removeRow(index)}
                  disabled={busy}
                  aria-label={`删除成员 ${index + 1}`}
                >
                  移除
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            className="button button-secondary"
            onClick={addRow}
            disabled={busy}
          >
            添加成员
          </button>
        </fieldset>
      ) : null}

      {"SortedSet" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Sorted Set 成员</legend>
          <div className="editor-rows">
            {draft.SortedSet.members.map((entry, index) => (
              <div className="editor-row editor-row-zset" key={`zset-${index}`}>
                <div className="module-tab-content">
                <label className="field">
                  <span>成员 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`成员 ${index + 1}`}
                    value={draftText(entry.member)}
                    onChange={(event) =>
                      updateSortedSetMember(index, "member", event.target.value)
                    }
                    disabled={busy}
                  />
                </label>
                {encodingSelect(`sorted-set.${index}`, `成员 ${index + 1}`, entry.member, (next) => updateSortedSetMember(index, "member", next))}
                </div>
                <label className="field">
                  <span>分数 {index + 1}</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    type="text"
                    inputMode="decimal"
                    aria-label={`分数 ${index + 1}`}
                    value={Number.isNaN(entry.score) ? "" : String(entry.score)}
                    onChange={(event) =>
                      updateSortedSetMember(index, "score", event.target.value)
                    }
                    disabled={busy}
                  />
                </label>
                <button
                  type="button"
                  className="button button-quiet row-remove"
                  onClick={() => removeRow(index)}
                  disabled={busy}
                  aria-label={`删除成员 ${index + 1}`}
                >
                  移除
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            className="button button-secondary"
            onClick={addRow}
            disabled={busy}
          >
            添加成员
          </button>
        </fieldset>
      ) : null}

      {"Json" in draft ? (
        <label className="field">
          <span>JSON 文档</span>
          <textarea
            autoCapitalize="off"
            autoCorrect="off"
            aria-label="JSON 文档"
            value={jsonDraft}
            onChange={(event) => {
              setJsonDraft(event.target.value);
              setValidationError(null);
            }}
            disabled={busy}
            spellCheck={false}
          />
        </label>
      ) : null}

      {"Stream" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Stream 条目</legend>
          <div className="editor-rows">
            {draft.Stream.entries.map((entry, entryIndex) => (
              <fieldset className="stream-entry" key={`stream-${entryIndex}`}>
                <legend>条目 {entryIndex + 1}</legend>
                <label className="field">
                  <span>ID</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label={`Stream 条目 ${entryIndex + 1} ID`}
                    value={entry.id}
                    onChange={(event) => updateStreamEntryId(entryIndex, event.target.value)}
                    disabled={busy}
                  />
                </label>
                {entry.fields.map((field, fieldIndex) => (
                  <div className="editor-row editor-row-hash" key={`stream-${entryIndex}-${fieldIndex}`}>
                    <label className="field">
                      <span>字段 {fieldIndex + 1} 名称</span>
                      <input
                        autoCapitalize="off"
                        autoCorrect="off"
                        aria-label={`Stream 字段 ${fieldIndex + 1} 名称`}
                        value={field.field}
                        onChange={(event) => updateStreamEntry(entryIndex, fieldIndex, "field", event.target.value)}
                        disabled={busy}
                      />
                    </label>
                    <label className="field">
                      <span>字段 {fieldIndex + 1} 值</span>
                      <input
                        autoCapitalize="off"
                        autoCorrect="off"
                        aria-label={`Stream 字段 ${fieldIndex + 1} 值`}
                        value={field.value}
                        onChange={(event) => updateStreamEntry(entryIndex, fieldIndex, "value", event.target.value)}
                        disabled={busy}
                      />
                    </label>
                    <button
                      type="button"
                      className="button button-quiet row-remove"
                      onClick={() => {
                        if (!("Stream" in draft)) return;
                        setDraft({
                          Stream: {
                            entries: draft.Stream.entries.map((currentEntry, currentEntryIndex) =>
                              currentEntryIndex === entryIndex
                                ? {
                                    ...currentEntry,
                                    fields: currentEntry.fields.filter((_, currentFieldIndex) => currentFieldIndex !== fieldIndex),
                                  }
                                : currentEntry,
                            ),
                          },
                        });
                        setValidationError(null);
                      }}
                      disabled={busy}
                      aria-label={`移除 Stream 字段 ${fieldIndex + 1}`}
                    >
                      移除
                    </button>
                  </div>
                ))}
                <button
                  type="button"
                  className="button button-quiet"
                  onClick={() => {
                    if (!("Stream" in draft)) return;
                    setDraft({
                      Stream: {
                        entries: draft.Stream.entries.map((currentEntry, currentEntryIndex) =>
                          currentEntryIndex === entryIndex
                            ? { ...currentEntry, fields: [...currentEntry.fields, { field: "", value: "" }] }
                            : currentEntry,
                        ),
                      },
                    });
                    setValidationError(null);
                  }}
                  disabled={busy}
                >
                  添加字段
                </button>
                <button
                  type="button"
                  className="button button-quiet"
                  onClick={() => removeRow(entryIndex)}
                  disabled={busy}
                  aria-label={`移除 Stream 条目 ${entryIndex + 1}`}
                >
                  移除条目
                </button>
              </fieldset>
            ))}
          </div>
          <button type="button" className="button button-secondary" onClick={addRow} disabled={busy}>
            添加 Stream 条目
          </button>
        </fieldset>
      ) : null}

      {showKeyActions ? <div className="ttl-editor">
        <label className="field">
          <span>TTL（毫秒）</span>
          <input
            autoCapitalize="off"
            autoCorrect="off"
            type="number"
            min="0"
            step="1"
            inputMode="numeric"
            value={ttlDraft}
            onChange={(event) => {
              setTtlDraft(event.target.value);
              setValidationError(null);
            }}
            placeholder={ttlMs < 0 ? "当前为永久" : undefined}
            disabled={busy}
          />
        </label>
        <button
          type="button"
          className="button button-secondary"
          onClick={() => void handleTtl()}
          disabled={busy}
        >
          设置 TTL
        </button>
      </div> : null}

      {validationError || error ? <Toast
        kind="error"
        message={validationError ?? error ?? ""}
        onClose={validationError ? () => setValidationError(null) : undefined}
        resetKey={validationError ? validationErrorToken : error}
      /> : null}

      <div className="editor-actions">
        {showKeyActions ? <button
          type="button"
          className="button button-danger"
          onClick={() => void onDelete()}
          disabled={busy}
        >
          {busy ? "处理中…" : "删除"}
        </button> : null}
        <button type="submit" className="button button-primary" disabled={busy}>
          {busy ? "保存中…" : "保存"}
        </button>
      </div>
    </form>
  );
}

export default KeyEditor;
