import { useState } from "react";

import BrowserPage from "./features/browser/BrowserPage";
import ConnectionPage from "./features/connections/ConnectionPage";
import DatabasePage from "./features/database/DatabasePage";
import WorkbenchPage from "./features/workbench/WorkbenchPage";
import type { ConnectionProfile, Workspace } from "./lib/types";

type AppSection = "connections" | Workspace;

interface NavigationItem {
  id: AppSection;
  label: string;
  description: string;
  icon: "connections" | "browser" | "workbench" | "database";
}

const navigationItems: NavigationItem[] = [
  {
    id: "connections",
    label: "连接管理",
    description: "Connections",
    icon: "connections",
  },
  { id: "browser", label: "Browser", description: "键浏览", icon: "browser" },
  {
    id: "workbench",
    label: "Workbench",
    description: "命令工作台",
    icon: "workbench",
  },
  {
    id: "database",
    label: "Database",
    description: "实例概览",
    icon: "database",
  },
];

const sectionDescriptions: Record<AppSection, string> = {
  connections: "保存并管理本地 Redis 实例",
  browser: "使用 SCAN 浏览键和值",
  workbench: "直接执行 Redis 命令并查看返回值",
  database: "查看实例指标和数据库键空间",
};

function NavigationIcon({ type }: { type: NavigationItem["icon"] }) {
  if (type === "connections") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M8 7V4m8 3V4M5 8h14v3a5 5 0 0 1-5 5h-4a5 5 0 0 1-5-5V8Z" />
        <path d="M8 16v4m8-4v4M8 20h8" />
      </svg>
    );
  }

  if (type === "browser") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <rect x="4" y="4" width="16" height="16" rx="2" />
        <path d="M4 9h16M9 9v11M15 9v11" />
      </svg>
    );
  }

  if (type === "database") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <ellipse cx="12" cy="6" rx="7" ry="3" />
        <path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" />
      </svg>
    );
  }

  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="m8 8-4 4 4 4M16 8l4 4-4 4M14 5l-4 14" />
    </svg>
  );
}

export default function App() {
  const [activeProfile, setActiveProfile] = useState<ConnectionProfile | null>(null);
  const [activeSection, setActiveSection] = useState<AppSection>("connections");

  const handleOpenConnection = (profile: ConnectionProfile | null) => {
    setActiveProfile(profile);
    setActiveSection(profile ? "browser" : "connections");
  };

  const handleProfileChanged = (profile: ConnectionProfile) => {
    setActiveProfile((current) =>
      current?.id === profile.id ? profile : current,
    );
  };

  const activeNavigation = navigationItems.find((item) => item.id === activeSection);
  const currentSection = activeNavigation ?? navigationItems[0];
  const canAccessWorkspace = activeProfile !== null;

  return (
    <main className="app-shell">
      <aside className="app-sidebar" aria-label="产品侧边栏">
        <div className="app-brand">
          <span className="app-brand-mark" aria-hidden="true">
            R
          </span>
          <div>
            <h1>Redix</h1>
            <span>Redis desktop client</span>
          </div>
        </div>

        <nav aria-label="主导航" className="app-navigation">
          <p className="app-navigation-label">工作区</p>
          {navigationItems.map((item) => {
            const isAvailable = item.id === "connections" || canAccessWorkspace;
            const isActive = currentSection.id === item.id;
            return (
              <button
                type="button"
                className={`app-navigation-item${isActive ? " app-navigation-item-active" : ""}${
                  !isAvailable ? " app-navigation-item-disabled" : ""
                }`}
                key={item.id}
                aria-label={item.label}
                aria-current={isActive ? "page" : undefined}
                aria-disabled={!isAvailable}
                disabled={!isAvailable}
                onClick={() => setActiveSection(item.id)}
              >
                <span className="app-navigation-icon">
                  <NavigationIcon type={item.icon} />
                </span>
                <span className="app-navigation-copy">
                  <strong>{item.label}</strong>
                  <small>{item.description}</small>
                </span>
              </button>
            );
          })}
        </nav>

        <div className="app-sidebar-footer">
          <span className="app-status-dot" aria-hidden="true" />
          <span>本地模式</span>
          <small>v0.1.0 MVP</small>
        </div>
      </aside>

      <section className="app-main">
        <header className="app-header">
          <div className="app-header-title">
            <p className="app-kicker">REDIX / LOCAL DATA TOOL</p>
            <p className="app-header-name">{currentSection.label}</p>
            <p>{sectionDescriptions[currentSection.id]}</p>
          </div>
          <div className="app-header-context">
            <span
              className={`connection-indicator${
                activeProfile ? " connection-indicator-active" : ""
              }`}
            >
              <span className="connection-indicator-dot" aria-hidden="true" />
              {activeProfile ? "已连接" : "未连接"}
            </span>
            <span className="app-version">
              {activeProfile
                ? `${activeProfile.name} · ${activeProfile.host}:${activeProfile.port}`
                : "选择一个 Redis 实例开始"}
            </span>
          </div>
        </header>

        <section className="workspace" aria-label="当前工作区">
          {activeSection === "connections" || !activeProfile ? (
            <ConnectionPage onOpenConnection={handleOpenConnection} />
          ) : null}
          <p className="workspace-context" aria-live="polite">
            {activeProfile
              ? `当前连接：${activeProfile.name} · ${activeProfile.host}:${activeProfile.port}`
              : "请先连接 Redis 后使用工作区。"}
          </p>
          {activeProfile && activeSection === "browser" ? (
            <BrowserPage connectionId={activeProfile.id} />
          ) : null}
          {activeProfile && activeSection === "workbench" ? (
            <WorkbenchPage connectionId={activeProfile.id} />
          ) : null}
          {activeProfile && activeSection === "database" ? (
            <DatabasePage
              connectionId={activeProfile.id}
              activeDatabase={activeProfile.database}
              onProfileChanged={handleProfileChanged}
            />
          ) : null}
        </section>
      </section>
    </main>
  );
}
