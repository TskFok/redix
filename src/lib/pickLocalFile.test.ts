import { beforeEach, expect, it, vi } from "vitest";

const { openMock } = vi.hoisted(() => ({
  openMock: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openMock,
}));

import { pickLocalFile } from "./pickLocalFile";

beforeEach(() => {
  openMock.mockReset();
});

it("返回所选文件的绝对路径", async () => {
  openMock.mockResolvedValue("/Users/operator/.ssh/id_ed25519");
  await expect(pickLocalFile("选择 SSH 私钥文件")).resolves.toBe(
    "/Users/operator/.ssh/id_ed25519",
  );
  expect(openMock).toHaveBeenCalledWith({
    multiple: false,
    directory: false,
    title: "选择 SSH 私钥文件",
  });
});

it("取消选择或空路径时返回 null", async () => {
  openMock.mockResolvedValueOnce(null);
  await expect(pickLocalFile()).resolves.toBeNull();
  openMock.mockResolvedValueOnce("   ");
  await expect(pickLocalFile()).resolves.toBeNull();
  expect(openMock).toHaveBeenCalledWith({
    multiple: false,
    directory: false,
  });
});
