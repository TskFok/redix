# 依赖安全审计（2026-10-23）

本次检查 npm 全部依赖（含开发依赖）、Cargo.lock 中的跨平台依赖，以及 Tauri 官方安全公告。直接在现有 `main` 分支修复，未发布版本。

## 已确认的问题与修复版本

| 依赖 | 原锁定版本 | 修复版本 | 原因 |
| --- | --- | --- | --- |
| Vite | 7.3.6 | 7.3.7 | 开发服务器文件访问、跨源脚本执行及内置 launch-editor 命令注入问题 |
| Vitest / @vitest/mocker | 4.1.11 | 4.1.12 | Mock 注册代码注入、Windows 路径遍历 |
| source-map-js | 1.2.1 | 1.2.2 | 索引 source map 偏移可导致事件循环拒绝服务 |
| Tauri（Rust crate） | 2.11.5 | 2.11.6 | IPC channel fetch 缺少跨 WebView 访问控制 |
| rustls | 0.23.42 | 0.23.46 | 4 条 RustSec 公告：会话调试输出泄露密钥、TLS 1.3 加密层边界、Brotli 内存分配、后续连接密钥交换降级 |
| rustls-webpki | 0.103.13 | 0.103.16 | 证书验证可能接受已失信的中间证书 |
| event-listener | 5.4.1 | 5.4.2 | `StackSlot` 允许非 Send 数据跨线程，属于 unsound 告警 |
| chacha20 | 0.10.1 | 0.10.2 | 移除已撤回版本 |
| libssh2-sys | 0.3.2 | 0.3.3 | 移除已撤回版本 |

同时移除本项目直接使用、已停止维护的 `rustls-pemfile`，改用现有 `rustls::pki_types::pem::PemObject`。已用真实证书/私钥验证证书链顺序、PKCS#8/RSA/EC 私钥、空内容及格式错误处理；测试密钥仅供自动化夹具使用。

`package.json` 提高 Vite/Vitest 的最低版本，`Cargo.toml` 提高 Tauri/rustls 的最低版本。其余修复由锁文件固定，不添加忽略规则，不使用强制主版本升级。

## 公告来源

- [Tauri IPC channel 访问控制公告](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv)：本次 RustSec 扫描未包含此项，另行核对官方公告后修复。
- [Vite 内置 launch-editor Windows 命令注入](https://github.com/advisories/GHSA-587h-7wmj-h5pm)、[Vitest 代码注入](https://github.com/advisories/GHSA-qxg6-89mg-rqv8)、[Vitest Windows 路径遍历](https://github.com/advisories/GHSA-xp26-3454-mjgp)、[source-map-js 拒绝服务](https://github.com/advisories/GHSA-68fv-2mgg-jv7q)。
- rustls：[RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html)、[RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html)、[RUSTSEC-2026-0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html)、[RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html)。
- rustls-webpki：[RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html)；event-listener：[RUSTSEC-2026-0221](https://rustsec.org/advisories/RUSTSEC-2026-0221.html)。

## 剩余告警的处理边界

`cargo audit` 的漏洞计数不包含所有 informational 告警；即使退出码为 0，也不能称整个 Rust 依赖树没有安全或维护风险。

- `glib 0.18.5` 的 [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) 属于 **unsound**：迭代器涉及未定义行为。修复在 0.20+，当前 Tauri Linux GTK3 依赖链仍需要 0.18，不能通过兼容补丁升级消除。本次没有忽略该告警，也未将其标为已修复。
- `proc-macro-error`、5 个 `unic-*`、`zbus_names`、`zvariant`、`zvariant_derive` 的停维护告警需要上游依赖迁移。`unic-*` 可随更大范围的 Tauri 配套组件升级及 `urlpattern` 迁移消除，本次只做安全补丁更新，未将它们描述为无解。

## 验证

- npm 初始审计：4 个受影响包（2 high、2 moderate）；修复后全量审计：0。
- Cargo 初始审计：5 条漏洞、10 条停维护、2 条 unsound、2 条已撤回版本；修复后：0 条漏洞、9 条停维护、1 条 unsound、0 条已撤回版本。RustSec 数据库版本为 `7ad117a308b144f2f7661fa555e4db5db9c5586e`（1,349 条公告）。
- `npm ci` 成功；`npm test -- --maxWorkers=2`：73 文件、850 测试通过。
- `npm run build`、`npm run test:release`（38 测试）、`npm run check:non-cloud` 通过。
- `cargo check --offline --manifest-path src-tauri/Cargo.toml` 通过；`cargo test --locked` 全量 522 通过、0 失败、49 ignored（这些测试需要显式 Redis/SSH 等环境，未额外运行）。
- 全量测试后补充一项 mTLS 成功对照，最终 3 项 PEM 定向回归通过；覆盖匹配证书与 PKCS#8、PKCS#1、SEC1 私钥、证书链保留及错误边界。该新增成功用例不计入前述 522 项全量结果。
- `cargo fmt --check`、`git diff --check` 通过；独立代码复审发现的私钥测试盲点已修复并复核关闭。
- `npm run tauri -- build --debug --no-bundle --ci -- --locked` 通过，生成 macOS 调试二进制 `src-tauri/target/debug/redix`；未生成安装包、签名或发布。

前端测试保留两条既有 jsdom navigation 提示，构建保留 615.27 kB 主包体积提示；Rust 保留 Array 命令及测试辅助函数的既有 unused 警告。npm 11 提示三个安装脚本尚未批准；本次未改变脚本安全策略，已安装的平台二进制足以完成前端验证。

Cargo.lock 中部分 Windows/getrandom 依赖边也发生变化：Cargo 在同一依赖宽版本约束内选择了已有兼容版本，相关包自身版本没有改变。已核对注册表约束并通过 `--locked` 测试。

本机验证环境为 macOS、Node 25.5.0、npm 11.17.0、Rust 1.93.1、cargo-audit 0.22.1；未将本机结果外推为 Windows/Linux 原生验证通过。

后续可复查：

```sh
npm audit
cargo audit --file src-tauri/Cargo.lock
```
