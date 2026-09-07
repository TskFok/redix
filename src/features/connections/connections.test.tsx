import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ConnectionPage from "./ConnectionPage";
import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";

const {
  listConnectionsMock,
  saveConnectionMock,
  openConnectionMock,
  closeConnectionMock,
  deleteConnectionMock,
  testConnectionMock,
  exportConnectionsMock,
  importConnectionsMock,
  onOpenConnectionMock,
} = vi.hoisted(() => ({
  listConnectionsMock: vi.fn(),
  saveConnectionMock: vi.fn(),
  openConnectionMock: vi.fn(),
  closeConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
  exportConnectionsMock: vi.fn(),
  importConnectionsMock: vi.fn(),
  onOpenConnectionMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  listConnections: listConnectionsMock,
  saveConnection: saveConnectionMock,
  openConnection: openConnectionMock,
  closeConnection: closeConnectionMock,
  deleteConnection: deleteConnectionMock,
  testConnection: testConnectionMock,
  exportConnections: exportConnectionsMock,
  importConnections: importConnectionsMock,
}));

const localProfile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
  tls: false,
  verify_server_cert: true,
  ca_certificate_name: null,
  client_certificate_name: null,
  has_ca_certificate: false,
  has_client_certificate: false,
};

const testCertificatePem =
  "-----BEGIN CERTIFICATE-----\ncertificate\n-----END CERTIFICATE-----";

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
    closeConnectionMock.mockResolvedValue(undefined);
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

  it("保存 Sentinel 种子与独立凭据并说明重新连接语义", async () => {
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => input.profile);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.change(screen.getByLabelText("连接拓扑"), { target: { value: "sentinel" } });
    fireEvent.change(screen.getByLabelText("Sentinel 主节点名称"), { target: { value: "primary" } });
    fireEvent.change(screen.getByLabelText("Sentinel 种子节点"), { target: { value: "sentinel-a:26379\n[::1]:26380" } });
    fireEvent.change(screen.getByLabelText("Sentinel 用户名"), { target: { value: "watcher" } });
    fireEvent.change(screen.getByLabelText("Sentinel 密码"), { target: { value: "sentinel-secret" } });
    fireEvent.change(screen.getByLabelText("密码"), { target: { value: "redis-secret" } });
    fireEvent.click(screen.getByLabelText("Sentinel 启用 TLS"));
    expect(screen.getByLabelText("启用 TLS")).not.toBeChecked();
    expect(screen.getByLabelText("验证服务端证书")).toBeInTheDocument();
    expect(screen.getByLabelText("CA 名称")).toBeInTheDocument();
    expect(screen.getByText(/重新连接时发现当前主节点/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
      profile: { sentinel: { master_name: "primary", nodes: [{ host: "sentinel-a", port: 26379 }, { host: "::1", port: 26380 }], username: "watcher", has_password: true } },
      password: "redis-secret", sentinel_password: "sentinel-secret",
    });
    expect(await screen.findByText("Sentinel · primary")).toBeInTheDocument();
  });

  it("SSH 路径仅进入 secret input 并保留当前组合门控", async () => {
    saveConnectionMock.mockImplementation(async (input: SaveConnectionInput) => input.profile);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByLabelText("启用 SSH 隧道"));
    fireEvent.change(screen.getByLabelText("SSH 主机"), { target: { value: "bastion.example" } });
    fireEvent.change(screen.getByLabelText("SSH 用户名"), { target: { value: "operator" } });
    fireEvent.change(screen.getByLabelText("SSH 私钥文件路径"), { target: { value: "/Users/operator/.ssh/id_ed25519" } });
    expect(screen.getByText(/known_hosts/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("启用 TLS"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH 暂不支持与 TLS 或 Sentinel 组合");
    expect(saveConnectionMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText("启用 TLS"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).toMatchObject({ host: "bastion.example", port: 22, username: "operator", auth_method: "agent", has_identity_file: true });
    expect(saveConnectionMock.mock.calls[0][0].ssh_identity_file).toBe("/Users/operator/.ssh/id_ed25519");
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).not.toHaveProperty("identity_file");
    expect(saveConnectionMock.mock.calls[0][0].profile.ssh).not.toHaveProperty("known_hosts_file");
    expect(await screen.findByText("SSH · bastion.example:22")).toBeInTheDocument();
  });

  it("导出连接时调用 typed IPC 并显示成功反馈", async () => {
    exportConnectionsMock.mockResolvedValue({ version: 2, connections: [] });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("还没有 Redis 连接");
    fireEvent.click(screen.getByRole("button", { name: "导出连接" }));

    await waitFor(() => expect(exportConnectionsMock).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("status")).toHaveTextContent("连接已导出");
  });

  it("导入 JSON 后追加连接并显示部分成功及敏感字段忽略提示", async () => {
    importConnectionsMock.mockResolvedValue({
      imported: [localProfile],
      failed: [
        {
          index: 1,
          name: "Broken",
          code: "INVALID_CONNECTION",
          message: "连接配置无效",
        },
      ],
      ignored_secret_fields: 1,
    });
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    const file = new File(
      ['{"connections":[]}'],
      "connections.json",
      { type: "application/json" },
    );
    fireEvent.change(screen.getByLabelText("导入连接文件"), {
      target: { files: [file] },
    });

    await waitFor(() => expect(importConnectionsMock).toHaveBeenCalledTimes(1));
    expect(importConnectionsMock).toHaveBeenCalledWith({
      content: '{"connections":[]}',
    });
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("部分导入");
    expect(status).toHaveTextContent("已忽略 1 个敏感字段");
    expect(status).toHaveTextContent("Broken");
  });

  it("拒绝超过 10 MiB 的连接导入文件", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    const file = new File(["{}"], "too-large.json", { type: "application/json" });
    Object.defineProperty(file, "size", { value: 10 * 1024 * 1024 + 1 });
    fireEvent.change(screen.getByLabelText("导入连接文件"), {
      target: { files: [file] },
    });

    expect(await screen.findByRole("alert")).toHaveTextContent("不能超过 10 MB");
    expect(importConnectionsMock).not.toHaveBeenCalled();
  });

  it("连接卡片显示 TLS 与证书重新录入状态", async () => {
    const tlsProfile: ConnectionProfile = {
      ...localProfile,
      id: "tls-profile",
      name: "TLS Redis",
      tls: true,
      ca_certificate_name: "Root CA",
      has_ca_certificate: false,
    };
    listConnectionsMock.mockResolvedValue([tlsProfile]);
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

    await screen.findByText("TLS Redis");
    expect(screen.getByText("已启用")).toBeInTheDocument();
    expect(screen.getByText("需重新录入")).toBeInTheDocument();
  });

  it("保存成功后刷新连接列表并按顺序打开对应连接", async () => {
    const calls: string[] = [];
    const saveInput: SaveConnectionInput = {
      clear_ssh_secrets: false,
      profile: localProfile,
      password: null,
      ca_certificate: null,
      client_certificate: null,
      client_key: null,
      clear_ca_certificate: false,
      clear_client_certificate: false,
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
      clear_ssh_secrets: false,
      password: null,
      ca_certificate: null,
      client_certificate: null,
      client_key: null,
      clear_ca_certificate: false,
      clear_client_certificate: false,
    });
  });

  it("保存 TLS 表单时发送校验开关和 CA 证书材料", async () => {
    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await openNewConnectionForm();
    fillStandaloneForm();
    fireEvent.click(screen.getByLabelText("启用 TLS"));
    fireEvent.change(screen.getByLabelText("CA 证书"), {
      target: { value: testCertificatePem },
    });
    fireEvent.change(screen.getByLabelText("CA 名称"), {
      target: { value: "Root CA" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
      profile: expect.objectContaining({
        tls: true,
        verify_server_cert: true,
        ca_certificate_name: "Root CA",
      }),
      ca_certificate: testCertificatePem,
      clear_ca_certificate: false,
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

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getByRole("button", { name: "删除 本地 Redis" }));
    await waitFor(() =>
      expect(onOpenConnectionMock).toHaveBeenNthCalledWith(2, null),
    );
  });

  it("切换连接时先打开新连接，再关闭旧连接并更新活动连接", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote",
      name: "远程 Redis",
      host: "192.0.2.10",
    };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`open:${connectionId}`);
      return { server_version: "8.4.0" };
    });
    closeConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`close:${connectionId}`);
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("远程 Redis");

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() =>
      expect(onOpenConnectionMock).toHaveBeenLastCalledWith(secondProfile),
    );

    expect(calls).toEqual(["open:local", "open:remote", "close:local"]);
    expect(closeConnectionMock).toHaveBeenCalledWith("local");
  });

  it("打开新连接失败时保留旧连接且不关闭旧连接", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote-failed",
      name: "失败 Redis",
    };
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      if (connectionId === secondProfile.id) {
        throw { code: "CONNECTION_FAILED", message: "连接失败" };
      }
      return { server_version: "8.4.0" };
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("失败 Redis");
    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("无法连接到 Redis 服务器"),
    );

    expect(closeConnectionMock).not.toHaveBeenCalled();
    expect(onOpenConnectionMock).toHaveBeenLastCalledWith(localProfile);
    expect(screen.getByRole("button", { name: "重新连接" })).toBeInTheDocument();
  });

  it("旧连接关闭失败时清理新连接并保留旧连接状态", async () => {
    const secondProfile: ConnectionProfile = {
      ...localProfile,
      id: "remote-close-failed",
      name: "关闭失败 Redis",
    };
    const calls: string[] = [];
    listConnectionsMock.mockResolvedValue([localProfile, secondProfile]);
    openConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`open:${connectionId}`);
      return { server_version: "8.4.0" };
    });
    closeConnectionMock.mockImplementation(async (connectionId: string) => {
      calls.push(`close:${connectionId}`);
      if (connectionId === localProfile.id) {
        throw { code: "CONNECTION_FAILED", message: "close failed" };
      }
    });

    render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
    await screen.findByText("关闭失败 Redis");
    fireEvent.click(screen.getAllByRole("button", { name: "连接" })[0]);
    await waitFor(() => expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile));

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("切换连接失败，已保留原连接。"),
    );

    expect(calls).toEqual([
      "open:local",
      "open:remote-close-failed",
      "close:local",
      "close:remote-close-failed",
    ]);
    expect(onOpenConnectionMock).toHaveBeenLastCalledWith(localProfile);
    expect(screen.getByRole("button", { name: "重新连接" })).toBeInTheDocument();
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
