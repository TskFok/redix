import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";
import { decodeStringValue, encodeStringValue, getStringValue, setStringValue } from "./valueCodecApi";
import { isStructuredFormat } from "./codecFormats";
import type { StringValue, StringValueSaved, ValueCompression, ValueFormat } from "./valueCodecApi";
import { browserErrorMessage } from "./browserState";
export interface StringValueEditorProps {
  connectionId: string;
  keyName: string;
  disabled?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onSaved?: (result: StringValueSaved) => void;
}
const FORMATS: [ValueFormat, string][] = [
  ["utf8", "UTF-8"], ["json", "JSON"], ["ascii", "ASCII"], ["hex", "Hex"],
  ["binary", "Binary"], ["base64", "Base64"], ["msgpack", "MessagePack / LZ4（只读）"],
  ["protobuf", "Protobuf（无 schema，只读）"],
  ["php", "PHP serialized（只读）"],
];
const COMPRESSIONS: [ValueCompression, string][] = [["none", "无压缩"], ["gzip", "Gzip"], ["zlib", "Zlib"], ["deflate", "Raw Deflate"]];

export default function StringValueEditor(props: StringValueEditorProps) {
  return <StringValueEditorScope key={JSON.stringify([props.connectionId, props.keyName])} {...props} />;
}

function StringValueEditorScope({ connectionId, keyName, disabled = false, onBusyChange, onSaved }: StringValueEditorProps) {
  const [raw, setRaw] = useState<StringValue | null>(null);
  const [format, setFormat] = useState<ValueFormat>("utf8");
  const [compression, setCompression] = useState<ValueCompression>("none");
  const [draft, setDraft] = useState("");
  const [decoded, setDecoded] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [hasDecoded, setHasDecoded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const generation = useRef(0);
  const writing = useRef(false);
  const mounted = useRef(false);
  const busyCallback = useRef(onBusyChange);
  busyCallback.current = onBusyChange;
  const dirty = draft !== decoded;
  const busy = loading || saving;
  const readOnly = raw?.truncated || isStructuredFormat(format);

  useEffect(() => { busyCallback.current?.(busy); }, [busy]);
  useEffect(() => {
    mounted.current = true;
    void load();
    return () => { mounted.current = false; generation.current += 1; busyCallback.current?.(false); };
  }, []);

  async function show(value: StringValue, nextFormat: ValueFormat, nextCompression: ValueCompression, token: number, fallback = false) {
    let result;
    let actualFormat = nextFormat;
    try {
      result = await decodeStringValue({ base64: value.base64, format: nextFormat, compression: nextCompression });
    } catch (reason) {
      if (!fallback || nextFormat !== "utf8" || nextCompression !== "none") throw reason;
      actualFormat = "hex";
      result = await decodeStringValue({ base64: value.base64, format: "hex", compression: "none" });
    }
    if (!mounted.current || generation.current !== token) return;
    setFormat(actualFormat); setCompression(nextCompression);
    setDraft(result.text); setDecoded(result.text); setHasDecoded(true);
  }

  async function load() {
    const token = ++generation.current;
    setLoading(true); setError(null); setNotice(null); setHasDecoded(false);
    try {
      const value = await getStringValue({ connection_id: connectionId, key: keyName });
      if (!mounted.current || generation.current !== token) return;
      setRaw(value);
      await show(value, format, compression, token, true);
    } catch (reason) {
      if (mounted.current && generation.current === token) setError(browserErrorMessage(reason, "读取或解码 String 失败，请切换格式或重试。"));
    } finally {
      if (mounted.current && generation.current === token) setLoading(false);
    }
  }

  async function changeView(nextFormat: ValueFormat, nextCompression: ValueCompression) {
    if (!raw || busy || disabled || dirty) return;
    const token = ++generation.current;
    setLoading(true); setError(null); setNotice(null); setHasDecoded(false);
    // A failed format remains selected so the user can explicitly choose another format.
    setFormat(nextFormat); setCompression(nextCompression); setDraft(""); setDecoded("");
    try { await show(raw, nextFormat, nextCompression, token); }
    catch { if (mounted.current && token === generation.current) setError("无法按所选格式解码，或数据超过解码限额。原始字节已保留，请切换 Hex 或 Base64 查看。"); }
    finally { if (mounted.current && token === generation.current) setLoading(false); }
  }

  async function save() {
    if (disabled || busy || writing.current || readOnly || !raw || !hasDecoded || !dirty) return;
    const token = generation.current;
    writing.current = true; setSaving(true); setError(null); setNotice(null);
    try {
      const base64 = await encodeStringValue({ text: draft, format, compression });
      // Switching away while a local encoder runs must not dispatch a Redis mutation.
      if (!mounted.current || generation.current !== token) return;
      const result = await setStringValue({ connection_id: connectionId, key: keyName, base64 });
      if (!mounted.current || generation.current !== token) return;
      setRaw({ base64, total_bytes: result.byte_length, ttl_ms: result.ttl_ms, truncated: false });
      setDecoded(draft); setNotice("值已保存，保留原 TTL。"); onSaved?.(result);
    } catch (reason) {
      if (mounted.current && generation.current === token) setError(browserErrorMessage(reason, "值格式无效或保存失败，草稿已保留。"));
    } finally {
      if (mounted.current && generation.current === token) { writing.current = false; setSaving(false); }
    }
  }

  return <section className="module-details string-value-editor" aria-label="String 解码器" aria-busy={busy}>
    <div className="string-value-toolbar">
      <label className="field"><span>值格式</span><Select aria-label="值格式" value={format} disabled={disabled || busy || dirty || !raw} onChange={(event) => void changeView(event.target.value as ValueFormat, compression)}>
        {FORMATS.map(([value, label]) => <option key={value} value={value} disabled={!!raw?.truncated && (isStructuredFormat(value) || value === "json")}>{label}</option>)}
      </Select></label>
      <label className="field"><span>压缩格式</span><Select aria-label="压缩格式" value={compression} disabled={disabled || busy || dirty || !raw || raw.truncated} onChange={(event) => void changeView(format, event.target.value as ValueCompression)}>
        {COMPRESSIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </Select></label>
      <button type="button" className="button button-secondary" onClick={() => void load()} disabled={disabled || busy || dirty}>重新读取</button>
    </div>
    {raw && <p className="feedback">原始值 {raw.total_bytes.toLocaleString()} 字节{raw.truncated ? "；仅预览前 4 MiB，已禁用保存，避免覆盖完整值。" : "；解码与解压在本机执行。"}</p>}
    {format === "protobuf" && <p className="feedback">按字段编号推测内容；没有 schema 时，字符串、字节和数值类型无法唯一确定。原始字节可在 Hex / Base64 中查看。</p>}
    {format === "msgpack" && <p className="feedback">支持标准 MessagePack 与 MessagePack-CSharp LZ4；解码结果只读，原始字节可在 Hex / Base64 中编辑。</p>}
    {format === "php" && <p className="feedback">PHP 对象、枚举、引用和自定义序列化内容按带标记的数据展示，不创建类实例或展开引用。gzcompress 数据可选择 Zlib 解压；原始字节可在 Hex / Base64 中查看。</p>}
    {loading && <p>正在读取和解码 String…</p>}
    {hasDecoded && <label className="field"><span>String 值</span><textarea aria-label="String 值" rows={12} value={draft} spellCheck={false} readOnly={!!readOnly} disabled={disabled || busy} onChange={(event) => { setDraft(event.target.value); setError(null); setNotice(null); }} /></label>}
    {dirty && <p className="feedback">草稿未保存；保存或放弃修改后可切换格式。</p>}
    {error && <p className="feedback feedback-error" role="alert">{error}</p>}
    {notice && <p className="feedback" role="status">{notice}</p>}
    <div className="editor-actions">
      <button type="button" className="button button-primary" onClick={() => void save()} disabled={disabled || busy || !!readOnly || !hasDecoded || !dirty}>保存值</button>
      <button type="button" className="button button-secondary" disabled={disabled || busy || !dirty} onClick={() => { setDraft(decoded); setError(null); setNotice(null); }}>放弃修改</button>
    </div>
  </section>;
}
