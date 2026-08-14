import type { Workspace } from "./lib/types";

const workspaces: Array<{ id: Workspace; label: string }> = [
  { id: "browser", label: "Browser" },
  { id: "workbench", label: "Workbench" },
];

export default function App() {
  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>Redix</h1>
      </header>
      <nav aria-label="工作区导航" className="workspace-tabs">
        {workspaces.map((workspace) => (
          <span key={workspace.id}>{workspace.label}</span>
        ))}
      </nav>
      <section className="workspace" aria-label="默认工作区">
        <p>选择一个工作区开始。</p>
      </section>
    </main>
  );
}
