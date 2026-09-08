import { useState } from "react";
import { saveConnectionTags, type ConnectionTag, type ConnectionTags } from "../../lib/localProductsApi";
import type { ConnectionProfile } from "../../lib/types";

export function connectionTagsFor(tags: ConnectionTags, connectionId: string): ConnectionTag[] {
  const value = Object.hasOwn(tags, connectionId) ? tags[connectionId] : undefined;
  return Array.isArray(value) ? value : [];
}

export function filterConnectionsByTag(profiles: ConnectionProfile[], tags: ConnectionTags, query: string, untagged = false) {
  const search = query.trim().toLocaleLowerCase();
  return profiles.filter((profile) => {
    const values = connectionTagsFor(tags, profile.id);
    return (!untagged || values.length === 0) && (!search || values.some((tag) => `${tag.key}=${tag.value}`.toLocaleLowerCase().includes(search)));
  });
}

export function ConnectionTagsPanel({ connectionId, connectionName, tags, onSaved }: {
  connectionId: string; connectionName: string; tags: ConnectionTag[];
  onSaved(id: string, tags: ConnectionTag[]): void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<ConnectionTag[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    const normalized = draft.map(({key,value}) => ({key:key.trim(),value:value.trim()}));
    if (normalized.some(tag => !tag.key || !tag.value || tag.key.length > 40 || tag.value.length > 120) || new Set(normalized.map(tag => tag.key)).size !== normalized.length) {
      setError("标签键和值不能为空，键不能重复；键最多 40 个字符，值最多 120 个字符。"); return;
    }
    setBusy(true); setError(null);
    try { const saved = await saveConnectionTags(connectionId, normalized); onSaved(connectionId, saved); setEditing(false); }
    catch { setError("标签保存失败，请检查输入或本地存储后重试。"); }
    finally { setBusy(false); }
  };
  return <section aria-label={`${connectionName} 连接标签`}>
    {tags.length > 0 && <div className="query-library-tags">{tags.map(tag => <span key={tag.key}>{tag.key}={tag.value}</span>)}</div>}
    {!editing ? <button type="button" className="button button-quiet" onClick={() => {setDraft(tags.map(tag => ({...tag})));setError(null);setEditing(true);}}>管理标签 {connectionName}</button> :
      <div role="group" aria-label={`编辑 ${connectionName} 标签`} aria-busy={busy}>
        {draft.map((tag,index) => <div className="form-grid" key={index}>
          <label className="field"><span>标签键 {index+1}</span><input autoCapitalize="off" autoCorrect="off" maxLength={40} value={tag.key} disabled={busy} onChange={event => setDraft(current => current.map((item,i) => i === index ? {...item,key:event.target.value}:item))}/></label>
          <label className="field"><span>标签值 {index+1}</span><input autoCapitalize="off" autoCorrect="off" maxLength={120} value={tag.value} disabled={busy} onChange={event => setDraft(current => current.map((item,i) => i === index ? {...item,value:event.target.value}:item))}/></label>
          <button type="button" className="button button-quiet" disabled={busy} onClick={() => setDraft(current => current.filter((_,i) => i !== index))}>删除标签 {index+1}</button>
        </div>)}
        {error && <p role="alert" className="feedback feedback-error">{error}</p>}
        <div className="card-actions">
          <button type="button" className="button button-secondary" disabled={busy || draft.length >= 20} onClick={() => setDraft(current => [...current,{key:"",value:""}])}>添加标签</button>
          <button type="button" className="button button-primary" disabled={busy} onClick={() => void save()}>保存标签</button>
          <button type="button" className="button button-quiet" disabled={busy} onClick={() => setEditing(false)}>取消标签编辑</button>
        </div>
      </div>}
  </section>;
}
