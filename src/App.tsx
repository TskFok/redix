import BulkTaskPanel from "./features/tasks/BulkTaskPanel";
import { useEffect, useRef, useState } from "react";
import { version as appVersion } from "../package.json";

import BrowserPage from "./features/browser/BrowserPage";
import ConnectionPage from "./features/connections/ConnectionPage";
import { connectionAddress } from "./features/connections/connectionState";
import TopologyPage from "./features/topology/TopologyPage";
import DatabaseAnalysisPage from "./features/database-analysis/DatabaseAnalysisPage";
import DatabasePage from "./features/database/DatabasePage";
import ObservabilityPage from "./features/observability/ObservabilityPage";
import QueryLibraryPage from "./features/query-library/QueryLibraryPage";
import SearchPage from "./features/search/SearchPage";
import SettingsPage from "./features/settings/SettingsPage";
import WorkbenchPage from "./features/workbench/WorkbenchPage";
import CliPage from "./features/cli/CliPage";
import ShortcutPalette, { type ShortcutAction } from "./features/shortcuts/ShortcutPalette";
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
  { id: "topology", label: "Cluster 拓扑", description: "节点与槽位", icon: "database" },
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

const navigationShortcuts: Partial<Record<AppSection, ShortcutAction["shortcut"]>> = {
  connections: { key: "1", label: "Ctrl/Cmd+1" },
  browser: { key: "2", label: "Ctrl/Cmd+2" },
  workbench: { key: "3", label: "Ctrl/Cmd+3" },
  "query-library": { key: "4", label: "Ctrl/Cmd+4" },
  settings: { key: ",", label: "Ctrl/Cmd+," },
};

// These selectors identify existing primary inputs without synthesizing clicks
// or submissions. Focus is checked again when the action runs.
const workspaceInputSelectors: Partial<Record<AppSection, string>> = {
  browser: 'input[aria-describedby="key-filter-hint"]',
  workbench: "#redis-command-input",
  "search-query": 'input[aria-label="查询语句"]',
  cli: ".cli-page input",
  "query-library": 'input[aria-label="搜索已保存查询"]',
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

function AppBrand() {
  return (
    <div className="app-brand">
      <span className="app-brand-mark" aria-hidden="true">R</span>
      <div>
        <h1>Redix</h1>
        <span>Redis desktop client</span>
      </div>
    </div>
  );
}

export default function App() {
  const [activeProfile, setActiveProfile] = useState<ConnectionProfile | null>(null);
  const [connectionWorkspaceOpen, setConnectionWorkspaceOpen] = useState(false);
  const [activeSection, setActiveSection] = useState<AppSection>("connections");
  const [settings, setSettings] = useState<AppSettings>(() => ({
    ...DEFAULT_APP_SETTINGS,
  }));
  const [pendingWorkbenchCommand, setPendingWorkbenchCommand] = useState<string | null>(null);
  const workspace = useRef<HTMLElement>(null);
  const focusConnectionsOnReturn = useRef(false);

  useEffect(() => {
    if (focusConnectionsOnReturn.current && activeSection === "connections") {
      document.querySelector<HTMLButtonElement>('[data-app-section="connections"]')?.focus();
    }
    focusConnectionsOnReturn.current = false;
  }, [activeSection, connectionWorkspaceOpen]);

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
    setConnectionWorkspaceOpen(profile !== null);
    if (!profile) setPendingWorkbenchCommand(null);
    setActiveSection(profile ? "browser" : "connections");
  };

  const handleBackToConnections = () => {
    focusConnectionsOnReturn.current = true;
    setConnectionWorkspaceOpen(false);
    setPendingWorkbenchCommand(null);
    setActiveSection("connections");
  };

  const handleProfileChanged = (profile: ConnectionProfile) => {
    setActiveProfile((current) =>
      current?.id === profile.id ? profile : current,
    );
  };

  const activeNavigation = navigationItems.find((item) => item.id === activeSection);
  const currentSection = activeNavigation ?? navigationItems[0];
  const canAccessWorkspace = connectionWorkspaceOpen && activeProfile !== null;
  const canAccessLocalResources = (section: AppSection) =>
    section === "connections" || section === "query-library" || section === "settings";
  const showConnectionPage =
    activeSection === "connections" || (!canAccessWorkspace && !canAccessLocalResources(activeSection));
  const navigationUnavailable = (section: AppSection) => {
    if (!canAccessLocalResources(section) && !canAccessWorkspace) return "请先从连接管理打开 Redis 连接";
    if (section === "topology" && !activeProfile?.cluster) return "仅 Cluster 连接可用";
    return undefined;
  };
  const navigateTo = (section: AppSection) => {
    if (navigationUnavailable(section)) return;
    if (section === "connections") {
      handleBackToConnections();
    } else {
      setActiveSection(section);
    }
  };
  const primaryInput = () => {
    const selector = workspaceInputSelectors[activeSection];
    const element = selector ? workspace.current?.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector) : null;
    return element && !element.disabled ? element : null;
  };
  const shortcutActions: ShortcutAction[] = [
    ...navigationItems.map((item) => ({
      id: item.id,
      label: item.label,
      description: item.description,
      shortcut: navigationShortcuts[item.id],
      unavailable: () => navigationUnavailable(item.id),
      run: () => {
        if (navigationUnavailable(item.id)) return;
        navigateTo(item.id);
        document.querySelector<HTMLButtonElement>(`[data-app-section="${item.id}"]`)?.focus();
      },
    })),
    {
      id: "focus-input", label: "聚焦当前输入", description: "键过滤、查询或命令编辑器",
      shortcut: { key: "f", shift: true, label: "Ctrl/Cmd+Shift+F" },
      unavailable: () => primaryInput() ? undefined : "当前工作区没有可用的查询或命令输入",
      run: () => { primaryInput()?.focus(); },
    },
  ];

  return (
    <main className={`app-shell${canAccessWorkspace ? "" : " app-shell-home"}`}>
      {canAccessWorkspace ? (
      <aside className="app-sidebar" aria-label="产品侧边栏">
        <AppBrand />
        <button
          type="button"
          className="app-back-button"
          aria-label="返回连接管理"
          title="返回连接管理 (Ctrl/Cmd+1)"
          onClick={handleBackToConnections}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <path d="m12 5-7 7 7 7M5 12h14" />
          </svg>
          <span className="app-back-label">返回连接管理</span>
        </button>

        <nav aria-label="主导航" className="app-navigation">
          <p className="app-navigation-label">工作区</p>
          {navigationItems.filter((item) => item.id !== "connections" && (item.id !== "topology" || activeProfile?.cluster)).map((item) => {
            const isAvailable = canAccessLocalResources(item.id) || canAccessWorkspace;
            const isActive = currentSection.id === item.id;
            return (
              <button
                type="button"
                className={`app-navigation-item${isActive ? " app-navigation-item-active" : ""}${
                  !isAvailable ? " app-navigation-item-disabled" : ""
                }`}
                key={item.id}
                data-app-section={item.id}
                aria-label={item.label}
                aria-current={isActive ? "page" : undefined}
                aria-disabled={!isAvailable}
                disabled={!isAvailable}
                onClick={() => navigateTo(item.id)}
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

        <div className="app-sidebar-actions">
          <ShortcutPalette actions={shortcutActions} />
        </div>

        <div className="app-sidebar-footer">
          <span className="app-status-dot" aria-hidden="true" />
          <span>本地模式</span>
          <small>v{appVersion} MVP</small>
        </div>
      </aside>
      ) : (
        <header className="app-home-header">
          <AppBrand />
          <div className="app-home-actions">
            <nav className="app-home-navigation" aria-label="主导航">
              {navigationItems.filter((item) => canAccessLocalResources(item.id)).map((item) => (
                <button
                  type="button"
                  key={item.id}
                  className={`button button-quiet${currentSection.id === item.id ? " app-home-navigation-active" : ""}`}
                  data-app-section={item.id}
                  aria-current={currentSection.id === item.id ? "page" : undefined}
                  onClick={() => navigateTo(item.id)}
                >
                  {item.label}
                </button>
              ))}
            </nav>
            <ShortcutPalette actions={shortcutActions} />
          </div>
        </header>
      )}

      <section className="app-main">
        <section ref={workspace} className={`workspace${canAccessWorkspace ? "" : " workspace-home"}`} aria-label={canAccessWorkspace ? "当前工作区" : "本地页面"}>
          {showConnectionPage ? (
            <ConnectionPage
              activeConnectionId={activeProfile?.id ?? null}
              onOpenConnection={handleOpenConnection}
            />
          ) : null}
          {canAccessWorkspace && activeProfile ? (
            <p className="workspace-context" aria-live="polite">
              {`当前连接：${activeProfile.name} · ${connectionAddress(activeProfile)}`}
            </p>
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "browser" ? (
            <BrowserPage connectionId={activeProfile.id} scanCount={settings.scan_count} />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "search-query" ? (
            <SearchPage connectionId={activeProfile.id} />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "workbench" ? (
            <WorkbenchPage
              connectionId={activeProfile.id}
              defaultFormat={settings.result_format}
              defaultContinueOnError={settings.continue_on_error}
              initialCommand={pendingWorkbenchCommand ?? undefined}
              onCommandConsumed={() => setPendingWorkbenchCommand(null)}
            />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "database" ? (
            <DatabasePage
              connectionId={activeProfile.id}
              activeDatabase={activeProfile.database}
              onProfileChanged={handleProfileChanged}
              isCluster={Boolean(activeProfile.cluster)}
              onOpenTopology={() => setActiveSection("topology")}
            />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "cli" ? (
            <CliPage connectionId={activeProfile.id} database={activeProfile.database} isCluster={Boolean(activeProfile.cluster)} />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "database-analysis" ? (
            <DatabaseAnalysisPage
              connectionId={activeProfile.id}
              activeDatabase={activeProfile.database}
            />
          ) : null}
          {canAccessWorkspace && activeProfile && activeSection === "observability" ? (
            <ObservabilityPage connectionId={activeProfile.id} isCluster={Boolean(activeProfile.cluster)} />
          ) : null}
          {canAccessWorkspace && activeProfile?.cluster && activeSection === "topology" ? <TopologyPage connectionId={activeProfile.id} /> : null}
          {activeSection === "query-library" ? (
            <QueryLibraryPage
              onFill={(command) => {
                setPendingWorkbenchCommand(command);
                setActiveSection(canAccessWorkspace ? "workbench" : "connections");
              }}
            />
          ) : null}
          {activeSection === "settings" ? (
            <SettingsPage settings={settings} onSaved={setSettings} />
          ) : null}
        </section>
        <BulkTaskPanel />
      </section>
    </main>
  );
}
