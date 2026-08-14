import { useState } from "react";

import BrowserPage from "./features/browser/BrowserPage";
import ConnectionPage from "./features/connections/ConnectionPage";
import WorkbenchPage from "./features/workbench/WorkbenchPage";
import type { ConnectionProfile, Workspace } from "./lib/types";

const workspaces: Array<{ id: Workspace; label: string }> = [
  { id: "browser", label: "Browser" },
  { id: "workbench", label: "Workbench" },
];

export default function App() {
  const [activeProfile, setActiveProfile] = useState<ConnectionProfile | null>(null);
  const [activeWorkspace, setActiveWorkspace] = useState<Workspace>("browser");

  const handleOpenConnection = (profile: ConnectionProfile | null) => {
    setActiveProfile(profile);
    if (!profile) {
      setActiveWorkspace("browser");
    }
  };

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <p className="app-kicker">REDIS WORKBENCH</p>
          <h1>Redix</h1>
        </div>
        <span className="app-version">桌面版 · MVP</span>
      </header>
      <nav aria-label="工作区导航" className="workspace-tabs">
        {workspaces.map((workspace) => {
          const isAvailable = activeProfile !== null;
          const isActive = activeWorkspace === workspace.id && isAvailable;
          return (
            <button
              type="button"
              className={`workspace-tab${isActive ? " workspace-tab-active" : ""}${
                !isAvailable ? " workspace-tab-placeholder" : ""
              }`}
              key={workspace.id}
              aria-current={isActive ? "page" : undefined}
              aria-disabled={!isAvailable}
              disabled={!isAvailable}
              onClick={() => setActiveWorkspace(workspace.id)}
            >
              {workspace.label}
            </button>
          );
        })}
      </nav>
      <section className="workspace" aria-label="当前工作区">
        <ConnectionPage onOpenConnection={handleOpenConnection} />
        <p className="workspace-context" aria-live="polite">
          {activeProfile
            ? `当前连接：${activeProfile.name} · ${activeProfile.host}:${activeProfile.port}`
            : "请先连接 Redis 后使用工作区。"}
        </p>
        {activeProfile && activeWorkspace === "browser" ? (
          <BrowserPage connectionId={activeProfile.id} />
        ) : null}
        {activeProfile && activeWorkspace === "workbench" ? (
          <WorkbenchPage connectionId={activeProfile.id} />
        ) : null}
      </section>
    </main>
  );
}
