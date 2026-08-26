# Task 4 报告：Browser RedisJSON 路径编辑器

## 实现文件

- `src/features/browser/JsonPathEditor.tsx`
- `src/features/browser/JsonPathEditor.test.tsx`
- `src/features/browser/BrowserPage.tsx`
- `src/features/browser/KeyDetails.tsx`
- `src/features/browser/browserState.ts`
- `src/features/browser/browser.test.tsx`
- `src/styles.css`

## RED

命令：

```bash
npm run test:frontend -- src/features/browser/JsonPathEditor.test.tsx src/features/browser/browser.test.tsx
```

关键输出：

```text
FAIL src/features/browser/JsonPathEditor.test.tsx
Error: Failed to resolve import "./JsonPathEditor"

src/features/browser/browser.test.tsx (35 tests | 4 failed)
× 加载页面时探测模块能力且不阻塞初始扫描
× 模块不可用时 JSON key 保留根编辑器并显示稳定降级提示
× JSON key 渲染路径编辑器，并在 mutation 成功后刷新详情
× BrowserPage 重命名后同步列表和当前选中键身份
```

结果符合 RED 预期：新组件尚不存在，Browser 还未接 capability probe 和路径编辑器。

## GREEN

命令：

```bash
npm run test:frontend -- src/features/browser/JsonPathEditor.test.tsx src/features/browser/browser.test.tsx
```

关键输出：

```text
Test Files  2 passed (2)
Tests  39 passed (39)
```

命令：

```bash
npm run build
```

关键输出：

```text
tsc -b && vite build
✓ 63 modules transformed.
✓ built
```

命令：

```bash
git diff --check
```

关键输出：无输出。

## 自审

- `JsonPathEditor` 只做表单、JSON parse/校验、read/mutate 回调，不访问 Tauri，也未引入 UI 依赖。
- `BrowserPage` 使用独立 `ModuleProbeState` 调 `getModuleCapabilities(connectionId)`；失败只降级路径编辑器，不阻塞初始 SCAN 和普通 Browser。
- `KeyDetails` 仅在 JSON key、probe ready、`json_supported` 为 true 时渲染路径编辑器；failed/unsupported 保留 root `KeyEditor` 并显示稳定降级提示。
- RedisJSON read/mutation 复用 `OperationContext` token、connectionId/key 校验和 `refreshDetail/onDetailChange`；切换连接、key、组件卸载时旧响应不会更新详情。
- 未改 Rust、IPC types/wrappers、README；未新增 Cloud/AI/Telemetry/SQL/KEYS/shell/远程插件。

## Concern

完整 `npm run test:frontend` 目前失败 5 个 out-of-scope 用例，原因是 `src/app.smoke.test.tsx` 和 `src/features/workbench/workbench.test.tsx` 的局部 Tauri mock 未导出 `getModuleCapabilities`，而这两个测试文件不在 Task 4 允许修改列表内。focused Browser 测试与 build 均通过。
