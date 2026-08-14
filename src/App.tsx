import { useState } from "react";

import ConnectionPage from "./features/connections/ConnectionPage";
import type { ConnectionProfile, Workspace } from "./lib/types";

const workspaces: Array<{ id: Workspace; label: string }> = [
  { id: "browser", label: "Browser" },
  { id: "workbench", label: "Workbench" },
];

export default function App() {
  const [activeProfile, setActiveProfile] = useState<ConnectionProfile | null>(null);

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <p className="app-kicker">REDIS WORKBENCH</p>
          <h1>Redix</h1>
        </div>
        <span className="app-version">桌面版 · MVP</span>
      </header>
      <nav aria-label="工作区导航（后续功能占位）" className="workspace-tabs">
        {workspaces.map((workspace) => (
          <span
            className={`workspace-tab${workspace.id === "browser" ? " workspace-tab-active" : ""}`}
            key={workspace.id}
            aria-current={workspace.id === "browser" ? "page" : undefined}
          >
            {workspace.label}
          </span>
        ))}
      </nav>
      <section className="workspace" aria-label="默认工作区">
        <ConnectionPage onOpenConnection={setActiveProfile} />
        <p className="workspace-context" aria-live="polite">
          {activeProfile
            ? `当前连接：${activeProfile.name} · ${activeProfile.host}:${activeProfile.port}`
            : "选择一个工作区开始。"}
        </p>
      </section>
    </main>
  );
}
