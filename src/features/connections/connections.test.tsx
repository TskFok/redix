import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ConnectionPage from "./ConnectionPage";
import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";

const {
  listConnectionsMock,
  saveConnectionMock,
  openConnectionMock,
  deleteConnectionMock,
  testConnectionMock,
  onOpenConnectionMock,
} = vi.hoisted(() => ({
  listConnectionsMock: vi.fn(),
  saveConnectionMock: vi.fn(),
  openConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
  onOpenConnectionMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  listConnections: listConnectionsMock,
  saveConnection: saveConnectionMock,
  openConnection: openConnectionMock,
  deleteConnection: deleteConnectionMock,
  testConnection: testConnectionMock,
}));

const localProfile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
};

function fillStandaloneForm() {
  fireEvent.change(screen.getByLabelText("连接名称"), {
    target: { value: "本地 Redis" },
  });
  fireEvent.change(screen.getByLabelText("主机"), {
    target: { value: "127.0.0.1" },
  });
  fireEvent.change(screen.getByLabelText("端口"), {
    target: { value: "6379" },
  });
}

async function openNewConnectionForm() {
  await screen.findByRole("button", { name: "新增连接" });
  fireEvent.click(screen.getByRole("button", { name: "新增连接" }));
  await screen.findByLabelText("连接名称");
}

describe("Redis 连接管理页面", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("crypto", { randomUUID: vi.fn(() => "local") });
    vi.stubGlobal("confirm", vi.fn(() => true));
    listConnectionsMock.mockResolvedValue([]);
    saveConnectionMock.mockResolvedValue(localProfile);
    openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
    deleteConnectionMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("无连接时显示新增提示并能打开连接表单", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    expect(await screen.findByText("还没有 Redis 连接")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "新增连接" }));

    expect(screen.getByLabelText("连接名称")).toBeInTheDocument();
    expect(screen.getByLabelText("主机")).toBeInTheDocument();
    expect(screen.getByLabelText("端口")).toBeInTheDocument();
    expect(screen.getByLabelText("密码")).toHaveAttribute("type", "password");
  });

  it("保存成功后刷新连接列表并按顺序打开对应连接", async () => {
    const calls: string[] = [];
    const saveInput: SaveConnectionInput = {
      profile: localProfile,
      password: null,
    };
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => {
      calls.push("save");
      expect(input).toEqual(saveInput);
      return localProfile;
    });
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push("open");
      expect(connectionId).toBe(localProfile.id);
      return { server_version: "8.4.0" };
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    await waitFor(() => {
      expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile);
    });
    expect(calls).toEqual(["save", "open"]);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);
    expect(openConnectionMock).toHaveBeenCalledWith(localProfile.id);
    expect(crypto.randomUUID).toHaveBeenCalled();
  });

  it("测试连接只调用 testConnection，不保存或打开连接", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "test-only-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(testConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock).not.toHaveBeenCalled();
    expect(openConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Redis 8.4.0");
  });

  it("提交前校验必填字段并在失败时显示可访问错误", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("请输入连接名称");
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });

  it("编辑已有连接时空密码会保留旧 secret 标记", async () => {
    const securedProfile: ConnectionProfile = {
      ...localProfile,
      id: "secured",
      name: "受保护 Redis",
      has_password: true,
    };
    listConnectionsMock.mockResolvedValue([securedProfile]);
    saveConnectionMock.mockResolvedValue(securedProfile);

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("受保护 Redis");
    fireEvent.click(screen.getByRole("button", { name: "编辑 受保护 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalledTimes(1));
    expect(saveConnectionMock).toHaveBeenCalledWith({
      profile: securedProfile,
      password: null,
    });
  });

  it("删除前确认，确认后删除并从列表移除连接", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("本地 Redis");

    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));

    await waitFor(() => expect(deleteConnectionMock).toHaveBeenCalledWith("local"));
    expect(screen.queryByText("本地 Redis")).not.toBeInTheDocument();
  });

  it("删除当前活动连接后通知父级清理活动 profile", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("本地 Redis");

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));
    await waitFor(() =>
      expect(onOpenConnectionMock).toHaveBeenNthCalledWith(2, null),
    );
  });

  it("保存成功但打开失败时保留 profile 并显示稳定提示", async () => {
    const calls: string[] = [];
    saveConnectionMock.mockImplementation(async () => {
      calls.push("save");
      return localProfile;
    });
    openConnectionMock.mockImplementation(async () => {
      calls.push("open");
      throw { code: "CONNECTION_FAILED", message: "无法连接到 Redis 服务器" };
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByRole("button", { name: "保存并连接" }));

    await waitFor(() => expect(screen.getByText("本地 Redis")).toBeInTheDocument());
    expect(calls).toEqual(["save", "open"]);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);
    expect(openConnectionMock).toHaveBeenCalledWith(localProfile.id);
    expect(onOpenConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "已保存连接，但打开失败，请重试",
    );
    expect(screen.queryByRole("heading", { name: "新增连接" })).not.toBeInTheDocument();
  });

  it("编辑已有认证连接空密码测试时提示输入密码", async () => {
    const securedProfile: ConnectionProfile = {
      ...localProfile,
      id: "secured-test",
      name: "受保护测试 Redis",
      has_password: true,
    };
    listConnectionsMock.mockResolvedValue([securedProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("受保护测试 Redis");
    fireEvent.click(screen.getByRole("button", { name: "编辑 受保护测试 Redis" }));
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "请输入密码后再测试连接",
    );
    expect(testConnectionMock).not.toHaveBeenCalled();
  });

  it("连接错误不会渲染密码、原始命令或 URI", async () => {
    saveConnectionMock.mockRejectedValue({
      code: "CONNECTION_FAILED",
      message: "redis://:secret-value@127.0.0.1:6379/0 执行 GET secret-key 失败",
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "secret-value" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法连接到 Redis 服务器");
    expect(alert).not.toHaveTextContent("secret-value");
    expect(alert).not.toHaveTextContent("redis://");
    expect(alert).not.toHaveTextContent("GET secret-key");
  });

  it("提交期间禁用重复保存操作", async () => {
    let resolveSave: ((profile: ConnectionProfile) => void) | undefined;
    saveConnectionMock.mockImplementation(
      () => new Promise<ConnectionProfile>((resolve) => {
        resolveSave = resolve;
      }),
    );
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    const saveButton = screen.getByRole("button", { name: "保存" });

    fireEvent.click(saveButton);
    expect(saveButton).toBeDisabled();
    fireEvent.click(saveButton);
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);

    resolveSave?.(localProfile);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument(),
    );
  });
});
