export default function App() {
  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>Redix</h1>
      </header>
      <nav aria-label="工作区导航" className="workspace-tabs">
        <span>Browser</span>
        <span>Workbench</span>
      </nav>
      <section className="workspace" aria-label="默认工作区">
        <p>选择一个工作区开始。</p>
      </section>
    </main>
  );
}
