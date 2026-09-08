import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import SettingsPage from "./SettingsPage";
import type { AppSettings } from "../../lib/types";

const { saveAppSettingsMock } = vi.hoisted(() => ({
  saveAppSettingsMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  saveAppSettings: saveAppSettingsMock,
}));

const settings: AppSettings = {
  version: 1,
  theme: "system",
  result_format: "text",
  scan_count: 100,
  continue_on_error: false,
};

describe("SettingsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    saveAppSettingsMock.mockResolvedValue({ ...settings, theme: "dark" });
  });

  afterEach(() => {
    cleanup();
  });

  it("渲染主题、结果格式、扫描数量和批量错误策略", () => {
    render(<SettingsPage settings={settings} onSaved={vi.fn()} />);

    expect(screen.getByLabelText("主题")).toHaveValue("system");
    expect(screen.getByLabelText("结果格式")).toHaveValue("text");
    expect(screen.getByLabelText("每次扫描数量")).toHaveValue(100);
    expect(screen.getByLabelText("批量命令遇错后继续")).not.toBeChecked();
  });

  it.each([5, 10001])("提交前拒绝越界扫描数量 %i，合法设置保存后回传", async (scanCount) => {
    const onSaved = vi.fn();
    render(<SettingsPage settings={settings} onSaved={onSaved} />);

    fireEvent.change(screen.getByLabelText("每次扫描数量"), {
      target: { value: String(scanCount) },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "扫描数量必须在 10 到 10000 之间。",
    );
    expect(saveAppSettingsMock).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("每次扫描数量"), {
      target: { value: "250" },
    });
    fireEvent.change(screen.getByLabelText("主题"), {
      target: { value: "dark" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    await waitFor(() => expect(saveAppSettingsMock).toHaveBeenCalledTimes(1));
    expect(saveAppSettingsMock).toHaveBeenCalledWith({
      ...settings,
      theme: "dark",
      scan_count: 250,
    });
    expect(onSaved).toHaveBeenCalledWith({ ...settings, theme: "dark" });
  });

  it.each([10, 1000, 10000])("允许保存扫描数量 %i 并将结果传回工作区", async (scanCount) => {
    saveAppSettingsMock.mockResolvedValue({ ...settings, scan_count: scanCount });
    const onSaved = vi.fn();
    render(<SettingsPage settings={settings} onSaved={onSaved} />);

    fireEvent.change(screen.getByLabelText("每次扫描数量"), {
      target: { value: String(scanCount) },
    });
    expect(screen.getByLabelText("每次扫描数量")).toBeValid();
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalledWith({ ...settings, scan_count: scanCount }));
    expect(saveAppSettingsMock).toHaveBeenCalledExactlyOnceWith({ ...settings, scan_count: scanCount });
    expect(screen.getByLabelText("每次扫描数量")).toHaveValue(scanCount);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("保存失败时显示固定错误而不显示底层文本", async () => {
    saveAppSettingsMock.mockRejectedValue({
      code: "PERSISTENCE_FAILED",
      message: "/Users/private/settings.json permission denied",
    });
    render(<SettingsPage settings={settings} onSaved={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "设置保存失败，请稍后重试。",
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent("private");
  });
});
