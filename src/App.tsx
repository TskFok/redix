import { useEffect, useState } from "react";

import BrowserPage from "./features/browser/BrowserPage";
import ConnectionPage from "./features/connections/ConnectionPage";
import DatabaseAnalysisPage from "./features/database-analysis/DatabaseAnalysisPage";
import DatabasePage from "./features/database/DatabasePage";
import ObservabilityPage from "./features/observability/ObservabilityPage";
import QueryLibraryPage from "./features/query-library/QueryLibraryPage";
import SearchPage from "./features/search/SearchPage";
import SettingsPage from "./features/settings/SettingsPage";
import WorkbenchPage from "./features/workbench/WorkbenchPage";
import CliPage from "./features/cli/CliPage";
import { getAppSettings } from "./lib/tauri";
import type { AppSettings, ConnectionProfile, Workspace } from "./lib/types";
import { DEFAULT_APP_SETTINGS } from "./features/settings/settingsState";

type AppSection = "connections" | Workspace;

interface NavigationItem {
  id: AppSection;
  label: string;
  description: string;
  icon:
    | "connections"
    | "browser"
    | "search-query"
    | "workbench"
    | "database"
    | "database-analysis"
    | "observability"
    | "query-library"
    | "settings";
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
    id: "search-query",
    label: "Search / Query",
    description: "索引与查询",
    icon: "search-query",
  },
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
  { id: "cli", label: "CLI", description: "持久命令会话", icon: "workbench" },
  {
    id: "database-analysis",
    label: "数据库分析",
    description: "键空间分析",
    icon: "database-analysis",
  },
  {
    id: "observability",
    label: "运维观察",
    description: "Slow Log / Pub/Sub / Profiler",
    icon: "observability",
  },
  {
    id: "query-library",
    label: "Query Library",
    description: "保存查询",
    icon: "query-library",
  },
  {
    id: "settings",
    label: "设置",
    description: "应用偏好",
    icon: "settings",
  },
];

const sectionDescriptions: Record<AppSection, string> = {
  connections: "保存并管理本地 Redis 实例",
  browser: "使用 SCAN 浏览键和值",
  "search-query": "管理 RedisSearch 索引并查询键",
  workbench: "直接执行 Redis 命令并查看返回值",
  cli: "独立持久连接与事务会话",
  database: "查看实例指标和数据库键空间",
  "database-analysis": "显式扫描并汇总键空间与内存",
  observability: "查看 Slow Log、Pub/Sub 和 Profiler",
  "query-library": "保存命令并回填 Workbench",
  settings: "调整主题和工作区偏好",
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

  if (type === "search-query") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <circle cx="10.5" cy="10.5" r="5.5" />
        <path d="m15 15 5 5" />
      </svg>
    );
  }

  if (type === "database" || type === "database-analysis") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <ellipse cx="12" cy="6" rx="7" ry="3" />
        <path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" />
      </svg>
    );
  }

  if (type === "query-library") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M6 4h12v16H6zM9 8h6M9 12h6M9 16h4" />
      </svg>
    );
  }

  if (type === "observability") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M4 17h3l2.3-6 3.1 9 2.4-7H20" />
        <path d="M4 5h16" />
      </svg>
    );
  }

  if (type === "settings") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="m9.5 4 .7-1h3.6l.7 1 1.2.7 1.2-.2 1.8 1.8-.2 1.2.7 1.2 1 .7v3.6l-1 .7-.7 1.2.2 1.2-1.8 1.8-1.2-.2-1.2.7-.7 1h-3.6l-.7-1-1.2-.7-1.2.2-1.8-1.8.2-1.2-.7-1.2-1-.7V9.4l1-.7.7-1.2-.2-1.2 1.8-1.8 1.2.2L9.5 4Z" />
        <circle cx="12" cy="11.5" r="2.5" />
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
  const [settings, setSettings] = useState<AppSettings>(() => ({
    ...DEFAULT_APP_SETTINGS,
  }));
  const [pendingWorkbenchCommand, setPendingWorkbenchCommand] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    void getAppSettings()
      .then((loaded) => {
        if (mounted) {
          setSettings(loaded);
        }
      })
      .catch(() => undefined);

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = settings.theme;
    root.classList.toggle("theme-light", settings.theme === "light");
    root.classList.toggle("theme-dark", settings.theme === "dark");
    return () => {
      root.classList.remove("theme-light", "theme-dark");
      delete root.dataset.theme;
    };
  }, [settings.theme]);

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
  const canAccessLocalResources = (section: AppSection) =>
    section === "connections" || section === "query-library" || section === "settings";
  const showConnectionPage =
    activeSection === "connections" || (!activeProfile && !canAccessLocalResources(activeSection));

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
            const isAvailable = canAccessLocalResources(item.id) || canAccessWorkspace;
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
          {showConnectionPage ? (
            <ConnectionPage onOpenConnection={handleOpenConnection} />
          ) : null}
          <p className="workspace-context" aria-live="polite">
            {activeProfile
              ? `当前连接：${activeProfile.name} · ${activeProfile.host}:${activeProfile.port}`
              : "请先连接 Redis 后使用工作区。"}
          </p>
          {activeProfile && activeSection === "browser" ? (
            <BrowserPage connectionId={activeProfile.id} scanCount={settings.scan_count} />
          ) : null}
          {activeProfile && activeSection === "search-query" ? (
            <SearchPage connectionId={activeProfile.id} />
          ) : null}
          {activeProfile && activeSection === "workbench" ? (
            <WorkbenchPage
              connectionId={activeProfile.id}
              defaultFormat={settings.result_format}
              defaultContinueOnError={settings.continue_on_error}
              initialCommand={pendingWorkbenchCommand ?? undefined}
              onCommandConsumed={() => setPendingWorkbenchCommand(null)}
            />
          ) : null}
          {activeProfile && activeSection === "database" ? (
            <DatabasePage
              connectionId={activeProfile.id}
              activeDatabase={activeProfile.database}
              onProfileChanged={handleProfileChanged}
            />
          ) : null}
          {activeProfile && activeSection === "cli" ? (
            <CliPage connectionId={activeProfile.id} database={activeProfile.database} />
          ) : null}
          {activeProfile && activeSection === "database-analysis" ? (
            <DatabaseAnalysisPage
              connectionId={activeProfile.id}
              activeDatabase={activeProfile.database}
            />
          ) : null}
          {activeProfile && activeSection === "observability" ? (
            <ObservabilityPage connectionId={activeProfile.id} />
          ) : null}
          {activeSection === "query-library" ? (
            <QueryLibraryPage
              onFill={(command) => {
                setPendingWorkbenchCommand(command);
                setActiveSection("workbench");
              }}
            />
          ) : null}
          {activeSection === "settings" ? (
            <SettingsPage settings={settings} onSaved={setSettings} />
          ) : null}
        </section>
      </section>
    </main>
  );
}
