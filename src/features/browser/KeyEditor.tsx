import { useEffect, useState } from "react";

import type { RedisValue } from "../../lib/types";
import {
  cloneRedisValue,
  redisValueKind,
  type RedisValueKind,
} from "./browserState";

interface KeyEditorProps {
  value: RedisValue;
  ttlMs: number;
  busy: boolean;
  error: string | null;
  onSave: (value: RedisValue) => Promise<void>;
  onDelete: () => Promise<void>;
  onSetTtl: (ttlMs: number) => Promise<void>;
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
  }
}

export function KeyEditor({
  value,
  ttlMs,
  busy,
  error,
  onSave,
  onDelete,
  onSetTtl,
}: KeyEditorProps) {
  const [draft, setDraft] = useState<RedisValue>(() => cloneRedisValue(value));
  const [ttlDraft, setTtlDraft] = useState(() => (ttlMs >= 0 ? String(ttlMs) : ""));
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(cloneRedisValue(value));
    setTtlDraft(ttlMs >= 0 ? String(ttlMs) : "");
    setValidationError(null);
  }, [ttlMs, value]);

  const kind = redisValueKind(draft);

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
    }
    setValidationError(null);
  };

  const removeRow = (index: number) => {
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
    }
    setValidationError(null);
  };

  const handleSave = async () => {
    let nextValue = cloneRedisValue(draft);
    if ("Set" in nextValue) {
      nextValue = {
        Set: { members: [...new Set(nextValue.Set.members)] },
      };
    }
    if (
      "SortedSet" in nextValue &&
      nextValue.SortedSet.members.some((entry) => !Number.isFinite(entry.score))
    ) {
      setValidationError("分数必须是有限数字。");
      return;
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
        <label className="field">
          <span>字符串值</span>
          <textarea
            value={draft.String.value}
            onChange={(event) => updateString(event.target.value)}
            disabled={busy}
            spellCheck={false}
          />
        </label>
      ) : null}

      {"Hash" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Hash 字段</legend>
          <div className="editor-rows">
            {draft.Hash.fields.map((entry, index) => (
              <div className="editor-row editor-row-hash" key={`hash-${index}`}>
                <label className="field">
                  <span>字段 {index + 1}</span>
                  <input
                    aria-label={`字段 ${index + 1}`}
                    value={entry.field}
                    onChange={(event) => updateHashField(index, "field", event.target.value)}
                    disabled={busy}
                  />
                </label>
                <label className="field">
                  <span>值 {index + 1}</span>
                  <input
                    aria-label={`值 ${index + 1}`}
                    value={entry.value}
                    onChange={(event) => updateHashField(index, "value", event.target.value)}
                    disabled={busy}
                  />
                </label>
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
                <label className="field">
                  <span>元素 {index + 1}</span>
                  <input
                    aria-label={`元素 ${index + 1}`}
                    value={item}
                    onChange={(event) => updateListItem(index, event.target.value)}
                    disabled={busy}
                  />
                </label>
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
                <label className="field">
                  <span>成员 {index + 1}</span>
                  <input
                    aria-label={`成员 ${index + 1}`}
                    value={member}
                    onChange={(event) => updateSetMember(index, event.target.value)}
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

      {"SortedSet" in draft ? (
        <fieldset className="editor-fieldset">
          <legend>Sorted Set 成员</legend>
          <div className="editor-rows">
            {draft.SortedSet.members.map((entry, index) => (
              <div className="editor-row editor-row-zset" key={`zset-${index}`}>
                <label className="field">
                  <span>成员 {index + 1}</span>
                  <input
                    aria-label={`成员 ${index + 1}`}
                    value={entry.member}
                    onChange={(event) =>
                      updateSortedSetMember(index, "member", event.target.value)
                    }
                    disabled={busy}
                  />
                </label>
                <label className="field">
                  <span>分数 {index + 1}</span>
                  <input
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

      <div className="ttl-editor">
        <label className="field">
          <span>TTL（毫秒）</span>
          <input
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
      </div>

      {validationError || error ? (
        <p className="feedback feedback-error" role="alert">
          {validationError ?? error}
        </p>
      ) : null}

      <div className="editor-actions">
        <button
          type="button"
          className="button button-danger"
          onClick={() => void onDelete()}
          disabled={busy}
        >
          {busy ? "处理中…" : "删除"}
        </button>
        <button type="submit" className="button button-primary" disabled={busy}>
          {busy ? "保存中…" : "保存"}
        </button>
      </div>
    </form>
  );
}

export default KeyEditor;
