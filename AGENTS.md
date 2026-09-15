# 项目协作规范

## 工作约定

- 始终使用简体中文回复。
- 默认在当前分支上修改；未经用户明确要求，不得新建分支。
- 禁止在循环遍历中查询 SQL。
- 启动桌面开发环境统一使用 `npm run tauri dev`。

## 提交信息规范

采用 Conventional Commits 格式，以英文 type 开头，具体描述使用简体中文，保持简短明确：

```text
<type>: <中文描述>
```

| type | 用途 |
| --- | --- |
| `feat` | 新功能 |
| `fix` | 修 bug |
| `refactor` | 重构 |
| `chore` | 日常维护 |
| `ci` | GitHub Actions / 部署配置 |
| `build` | 依赖 / 构建变更 |
| `docs` | 文档 |
| `test` | 测试 |

示例：

```text
feat: 支持连接分组
fix: 修复连接超时提示
docs: 补充发布规则
```

## Release Notes

- 配合 conventional-changelog 或 Release Drafter 自动生成 Release Notes，并按上述 type 分组。
- 使用 conventional-changelog 时，根据提交信息中的 type 分类。
- 使用 Release Drafter 时，PR 标题遵循相同格式，并将标题中的 type 映射为对应标签，再按标签分组。
