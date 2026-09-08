import { useEffect, useId, useRef, type ReactNode } from "react";

interface DetailTab {
  id: string;
  label: string;
  content: ReactNode;
}

interface KeyDetailsTabsProps {
  tabs: DetailTab[];
  selectedTab: string;
  onSelectTab: (tab: string) => void;
  feedback?: ReactNode;
}

export default function KeyDetailsTabs({ tabs, selectedTab, onSelectTab, feedback }: KeyDetailsTabsProps) {
  const id = useId();
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const activeTab = tabs.some((tab) => tab.id === selectedTab) ? selectedTab : "value";

  useEffect(() => {
    if (selectedTab !== activeTab) onSelectTab(activeTab);
  }, [activeTab, selectedTab, onSelectTab]);

  return (
    <>
      <div className="detail-tabs" role="tablist" aria-label="键详情分区">
        {tabs.map((tab, index) => (
          <button
            key={tab.id}
            ref={(button) => {
              if (button) buttons.current.set(tab.id, button);
              else buttons.current.delete(tab.id);
            }}
            type="button"
            role="tab"
            id={`${id}-tab-${tab.id}`}
            aria-controls={`${id}-panel-${tab.id}`}
            aria-selected={activeTab === tab.id}
            tabIndex={activeTab === tab.id ? 0 : -1}
            className="detail-tab"
            onClick={() => onSelectTab(tab.id)}
            onKeyDown={(event) => {
              let nextIndex: number;
              switch (event.key) {
                case "ArrowRight": nextIndex = (index + 1) % tabs.length; break;
                case "ArrowLeft": nextIndex = (index - 1 + tabs.length) % tabs.length; break;
                case "Home": nextIndex = 0; break;
                case "End": nextIndex = tabs.length - 1; break;
                default: return;
              }
              event.preventDefault();
              const nextTab = tabs[nextIndex].id;
              onSelectTab(nextTab);
              buttons.current.get(nextTab)?.focus();
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          role="tabpanel"
          id={`${id}-panel-${tab.id}`}
          aria-labelledby={`${id}-tab-${tab.id}`}
          className="detail-tab-panel"
          hidden={activeTab !== tab.id}
          tabIndex={0}
        >
          {/* Keep editors mounted so tab changes preserve drafts and in-flight operations. */}
          {activeTab === tab.id ? feedback : null}
          {tab.content}
        </div>
      ))}
    </>
  );
}
