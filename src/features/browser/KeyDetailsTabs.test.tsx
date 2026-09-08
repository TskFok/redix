import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { KeyInfo, KeyValue } from "../../lib/types";
import type { ModuleProbeState } from "./browserState";
import KeyDetails from "./KeyDetails";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const detail: KeyValue = {
  key: "profile:1",
  key_type: "ReJSON-RL",
  ttl_ms: 5000,
  value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
};

const moduleProbe: ModuleProbeState = {
  status: "ready",
  capabilities: {
    modules: [],
    json_supported: true,
    json_version: "20611",
    search_supported: false,
    search_version: null,
    array_supported: false,
    vector_set_supported: false,
  },
};

const props = {
  connectionId: "local",
  detail,
  loading: false,
  moduleProbe,
  onDetailChange: vi.fn(),
  onDeleted: vi.fn(),
};

describe("键详情标签页", () => {
  afterEach(() => {
    cleanup();
    vi.resetAllMocks();
  });

  it("切换分区只展示当前面板，并保留未保存的 JSON 文档", () => {
    render(<KeyDetails {...props} />);

    const tabList = screen.getByRole("tablist", { name: "键详情分区" });
    const valueTab = within(tabList).getByRole("tab", { name: "值", selected: true });
    const valuePanel = screen.getByRole("tabpanel", { name: "值" });
    const documentInput = within(valuePanel).getByLabelText("JSON 文档");
    expect(within(valuePanel).queryByRole("button", { name: "设置 TTL" })).not.toBeInTheDocument();
    expect(within(valuePanel).queryByRole("button", { name: /删除/ })).not.toBeInTheDocument();
    fireEvent.change(documentInput, { target: { value: '{"name":"未保存"}' } });

    fireEvent.click(within(tabList).getByRole("tab", { name: "元数据" }));
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    const metadataPanel = screen.getByRole("tabpanel", { name: "元数据" });
    expect(metadataPanel).toBeVisible();
    expect(within(metadataPanel).getByText("5000 ms")).toBeVisible();
    expect(within(metadataPanel).getByRole("button", { name: "刷新元数据" })).toBeVisible();
    expect(valuePanel).not.toBeVisible();
    expect(documentInput).toBeInTheDocument();

    fireEvent.click(within(tabList).getByRole("tab", { name: "键操作" }));
    const actionsPanel = screen.getByRole("tabpanel", { name: "键操作" });
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(within(actionsPanel).getByRole("textbox", { name: "新键名" })).toBeVisible();
    expect(within(actionsPanel).getByRole("button", { name: "设置 TTL" })).toBeVisible();
    expect(within(actionsPanel).getByRole("button", { name: /删除/ })).toBeVisible();
    expect(metadataPanel).not.toBeVisible();

    fireEvent.click(valueTab);
    expect(screen.getByRole("tabpanel", { name: "值" })).toBeVisible();
    expect(screen.getByLabelText("JSON 文档")).toBe(documentInput);
    expect(documentInput).toHaveValue('{"name":"未保存"}');
  });

  it.each([
    { change: "连接", nextProps: { connectionId: "remote", detail } },
    { change: "键", nextProps: { connectionId: "local", detail: { ...detail, key: "profile:2" } } },
    { change: "类型", nextProps: { connectionId: "local", detail: { ...detail, key_type: "ReJSON-RS" } } },
  ])("切换$change后回到值面板", ({ nextProps }) => {
    const { rerender } = render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "元数据" }));
    expect(screen.getByRole("tab", { name: "元数据", selected: true })).toBeInTheDocument();

    rerender(<KeyDetails {...props} {...nextProps} />);

    expect(screen.getByRole("tab", { name: "值", selected: true })).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "值" })).toBeVisible();
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
  });

  it("同键 TTL 刷新保持当前标签和路径草稿", () => {
    const { rerender } = render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    const pathInput = within(screen.getByRole("tabpanel", { name: "JSON Path" })).getByRole("textbox", { name: "JSON Path" });
    fireEvent.change(pathInput, { target: { value: "$.name" } });

    rerender(<KeyDetails {...props} detail={{ ...detail, ttl_ms: 4000 }} />);

    expect(screen.getByRole("tab", { name: "JSON Path", selected: true })).toBeInTheDocument();
    expect(pathInput).toBeVisible();
    expect(pathInput).toHaveValue("$.name");
  });

  it("同键重新加载后保留当前标签，加载其他键后回到值", () => {
    const { rerender } = render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "元数据" }));

    rerender(<KeyDetails {...props} detail={null} loading />);
    expect(screen.getByRole("status")).toHaveTextContent("正在读取键详情");
    rerender(<KeyDetails {...props} />);

    expect(screen.getByRole("tab", { name: "元数据", selected: true })).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "元数据" })).toBeVisible();

    rerender(<KeyDetails {...props} detail={null} loading />);
    rerender(<KeyDetails {...props} detail={{ ...detail, key: "profile:2" }} />);

    expect(screen.getByRole("tab", { name: "值", selected: true })).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "值" })).toBeVisible();
  });

  it("同键 TTL 更新后键操作输入同步显示最新 TTL", () => {
    const { rerender } = render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    expect(screen.getByRole("spinbutton", { name: "TTL（毫秒）" })).toHaveValue(5000);

    rerender(<KeyDetails {...props} detail={{ ...detail, ttl_ms: 4000 }} />);

    expect(screen.getByRole("tab", { name: "键操作", selected: true })).toBeInTheDocument();
    expect(screen.getByRole("spinbutton", { name: "TTL（毫秒）" })).toHaveValue(4000);
  });

  it("索引能力失效后回到值面板，恢复能力不会重新跳到索引", async () => {
    invokeMock.mockResolvedValue([]);
    const searchProbe: ModuleProbeState = {
      ...moduleProbe,
      capabilities: { ...moduleProbe.capabilities, search_supported: true, search_version: "20800" },
    };
    const { rerender } = render(<KeyDetails {...props} moduleProbe={searchProbe} />);
    await act(async () => {});
    fireEvent.click(screen.getByRole("tab", { name: "索引" }));
    expect(screen.getByRole("tabpanel", { name: "索引" })).toBeVisible();

    rerender(<KeyDetails {...props} />);
    expect(screen.queryByRole("tab", { name: "索引" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "值", selected: true })).toBeInTheDocument();
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);

    rerender(<KeyDetails {...props} moduleProbe={searchProbe} />);
    await act(async () => {});
    expect(screen.getByRole("tab", { name: "索引", selected: false })).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "值" })).toBeVisible();
  });

  it("RedisJSON 不可用时仍能打开路径标签查看原提示", () => {
    render(<KeyDetails {...props} moduleProbe={{
      ...moduleProbe,
      capabilities: { ...moduleProbe.capabilities, json_supported: false, json_version: null },
    }} />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));

    const pathPanel = screen.getByRole("tabpanel", { name: "JSON Path" });
    expect(within(pathPanel).getByRole("status")).toHaveTextContent("RedisJSON 路径编辑器暂不可用");
    expect(within(pathPanel).queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
  });

  it("左右方向键循环切换，Home 和 End 同时更新焦点及选中面板", () => {
    render(<KeyDetails {...props} />);
    const tabs = within(screen.getByRole("tablist", { name: "键详情分区" })).getAllByRole("tab");
    const first = tabs[0];
    const second = tabs[1];
    const last = tabs[tabs.length - 1];
    first.focus();

    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(second).toHaveFocus();
    expect(second).toHaveAttribute("aria-selected", "true");
    expect(first).toHaveAttribute("tabindex", "-1");

    fireEvent.keyDown(second, { key: "ArrowLeft" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: "ArrowLeft" });
    expect(last).toHaveFocus();
    expect(last).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(last, { key: "ArrowRight" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: "End" });
    expect(last).toHaveFocus();
    expect(screen.getByRole("tabpanel")).toHaveAttribute("id", last.getAttribute("aria-controls"));

    fireEvent.keyDown(last, { key: "Home" });
    expect(first).toHaveFocus();
    expect(first).toHaveAttribute("aria-selected", "true");
    expect(first).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tabpanel", { name: "值" })).toBeVisible();
  });

  it("请求处理中允许切换标签，但键操作继续禁用", async () => {
    let finishRequest!: (metadata: KeyInfo) => void;
    invokeMock.mockReturnValueOnce(new Promise<KeyInfo>((resolve) => { finishRequest = resolve; }));
    render(<KeyDetails {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: "元数据" }));
    fireEvent.click(screen.getByRole("button", { name: "刷新元数据" }));
    expect(screen.getByRole("button", { name: "刷新元数据" })).toBeDisabled();

    const actionsTab = screen.getByRole("tab", { name: "键操作" });
    expect(actionsTab).toBeEnabled();
    fireEvent.click(actionsTab);
    const actionsPanel = screen.getByRole("tabpanel", { name: "键操作" });
    expect(actionsPanel).toBeVisible();
    expect(within(actionsPanel).getByRole("button", { name: "重命名" })).toBeDisabled();
    expect(within(actionsPanel).getByRole("button", { name: "设置 TTL" })).toBeDisabled();

    await act(async () => {
      finishRequest({ key: detail.key, key_type: detail.key_type, ttl_ms: 5000, size: 2, memory_bytes: 96, encoding: "json", idle_seconds: 0 });
    });
    expect(within(actionsPanel).getByRole("button", { name: "重命名" })).toBeEnabled();
  });
});
