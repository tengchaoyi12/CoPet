# Codex 快捷操作设计

## 背景

CoPet 已能通过 Codex Hook 展示运行、等待、完成和失败提醒，但等待提醒目前只能把用户带回 Codex。两类常见场景仍有额外操作成本：

- Codex 已给出阶段结果，只需要用户确认继续，不需要补充信息或选择方案。
- Codex 正等待一次具体的工具执行权限。

本设计让用户直接在宠物提醒中查看任务、待执行动作和必要上下文，并在不切换应用的情况下继续原任务。

## 目标

- 普通“无需抉择的继续”显示固定位置的“继续执行”按钮。
- 工具权限请求显示具体工具、命令和工作目录，并提供“允许并继续”按钮。
- 权限按钮只批准当前这一条请求，不创建会话级或永久规则。
- 点击后继续原 Codex 会话和原轮次，不使用 macOS 界面自动化。
- 需要用户选择、输入信息或确认业务含义时，不显示快捷继续按钮。
- CoPet 不可用、动作过期或状态不确定时，绝不自动批准或自动继续。

## 非目标

- 不在 Codex 对话记录中伪造一条可见的用户消息“继续执行”。
- 不支持会话级、项目级或永久权限授权。
- 不自动回答 A/B 选择、参数输入、删除目标确认等需要用户判断的问题。
- 不依赖实验性的 App Server 注入，也不通过辅助功能操纵 Codex 窗口。
- 第一版不为未知工具或无法解释的高风险操作提供快捷批准。

## 方案选择

### 采用方案：双向阻塞式 Codex Hook

Codex 的 `PermissionRequest` Hook 可以返回一次性的 `allow` 或 `deny` 决定。`Stop` Hook 可以返回 `decision: "block"` 和原因，让同一轮任务继续执行。因此 CoPet 不需要向 Codex 输入框注入文字，也不需要获得 macOS 辅助功能权限。

CoPet Hook 将从单向事件上报扩展为有限的双向动作协议：

1. Hook 把原始 Codex Hook 输入提交到本地 runtime。
2. runtime 创建带唯一 `actionId` 的等待提醒。
3. Hook 在 Codex 允许的超时窗口内等待决定。
4. 用户点击快捷按钮后，Tauri 命令只解析该 `actionId`。
5. Hook 收到结果并向 Codex stdout 输出对应的原生 Hook JSON。

### 未采用方案

- App Server 或 `codex exec resume`：可以恢复会话，但不能可靠解析桌面端当前正在等待的原始权限请求，也可能产生并发轮次。
- macOS 辅助功能：能够点击界面，但依赖窗口层级、语言、版本和焦点状态，且需要额外系统权限。
- 自然语言全量推断：容易把需要选择或输入的内容误判为普通继续。

## 动作类型与界面

### 普通继续

- 状态文案：`任务等待继续`
- 内容：任务标题、阶段结果、Codex 声明的下一步。
- 主按钮：`继续执行`
- 次按钮：`在 Codex 中处理`

主按钮让 Stop Hook 返回：

```json
{
  "decision": "block",
  "reason": "用户已在 CoPet 确认继续执行。请继续完成当前任务，不要再次询问是否继续。"
}
```

Codex 随后在同一轮中继续。再次触发 Stop 时，`stop_hook_active` 为真，CoPet 必须直接放行，避免形成无限继续循环。

### 工具权限

- 状态文案：`任务等待执行权限`
- 内容：工具名、命令或目标、工作目录、Codex 提供的说明。
- 主按钮：`允许并继续`
- 辅助说明：`仅允许本次操作`
- 次按钮：`在 Codex 中处理`

主按钮让 PermissionRequest Hook 返回：

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "allow"
    }
  }
}
```

“在 Codex 中处理”或关闭提醒不会批准操作，而是让 Hook 返回 `{}`，使 Codex 回落到自身的原生审批流程。

### 需要用户抉择

以下情况不显示快捷按钮，只保留“打开 Codex”：

- 需要选择互斥方案。
- 需要输入缺失参数、凭证或业务内容。
- 需要确认删除对象、发布目标或不可逆操作。
- 请求会话级、项目级或永久权限。
- CoPet 无法确认动作内容或动作已过期。

## “无需抉择的继续”识别

第一版采用“显式标记 + 安全排除”，不依赖开放式语言模型分类。

CoPet 管理的 `UserPromptSubmit` Hook 向 Codex 注入一条短上下文：只有当任务无需用户提供信息、无需选择、无需权限审批且可以安全继续时，最终回复才附加隐藏标记：

```text
<!-- copet:continue -->
```

Stop Hook 仅在以下条件全部满足时创建快捷继续动作：

- `last_assistant_message` 包含完全匹配的标记。
- `stop_hook_active` 不为真。
- 会话标识和轮次标识存在。
- 文本未命中选择、输入、永久授权或破坏性确认的排除规则。

无法确认时按普通完成提醒处理。漏掉快捷按钮优于展示一个语义错误的按钮。

## 数据模型

`TaskNotification` 增加可选的 `action`：

```text
TaskAction
├── id                  一次性随机动作标识
├── kind                continue | permission
├── state               pending | resolving | expired
├── label               继续执行 | 允许并继续
├── requestedAction     面向用户的下一步说明
├── toolName            可选
├── command             可选，展示前遮罩敏感值
├── cwd                 可选
├── expiresAtMs
└── quickActionAllowed  是否允许直接操作
```

动作和提醒使用不同标识。提醒仍按 `agent + sessionId + turnId` 聚合；动作按随机 `actionId` 精确解析。新的 Hook 请求到来时，旧动作立即失效。

## 本地协议

runtime 继续只监听 `127.0.0.1`，并复用每次启动轮换的 Bearer token。

- `POST /v1/actions`：提交原始 Hook 输入，注册动作并返回 `actionId`。
- `GET /v1/actions/{id}/decision`：Hook 长轮询一次性决定。
- Tauri 命令 `resolve_task_action(id, decision)`：由用户点击触发。

HTTP 服务器按连接派生短生命周期工作线程。长轮询不能持有 `RuntimeCore` 锁，也不能阻塞其他 Agent 事件。决定一经读取即消费，重复读取或重复点击返回已处理。

## 生命周期与失败策略

默认动作有效期为十分钟，Hook 在 Codex 默认十分钟超时前主动结束：

- 普通继续超时：返回 `{}`，允许当前轮正常结束。
- 权限请求超时：返回 `{}`，回落到 Codex 原生审批。
- CoPet 未运行或 token 无效：Hook 快速返回 `{}`。
- CoPet 退出：所有等待者收到 fallback；不遗留允许决定。
- 用户关闭卡片：执行 fallback，不等同于拒绝或批准。
- 用户选择“在 Codex 中处理”：执行 fallback 后打开原任务。
- 用户双击：第一次成功，后续调用返回幂等结果。
- 新事件使旧动作失效：旧按钮返回“任务状态已变化”。

动作不跨 CoPet 重启持久化。提醒可以持久化，但恢复后的动作统一标记为过期，只能打开 Codex。

## 安全策略

- 权限决定只允许 `allow_once` 和 `fallback`，数据模型中不提供永久批准值。
- 第一版快捷批准仅覆盖内容可展示的本地工具权限。
- 未知工具、内容缺失或明显不可逆命令将 `quickActionAllowed` 设为假。
- 命令展示遮罩常见令牌、密码和授权头；完整原始输入只在内存中短暂保存。
- 日志只记录动作种类、会话和结果，不记录完整命令、凭证或 Hook payload。
- HTTP 路由校验 Bearer token、正文大小和动作状态。

## 与现有完成提醒的关系

Stop 快捷动作存在时，任务尚未真正完成，因此状态是 `waiting`，不能显示完成庆祝文案。用户点击继续后状态转为 `running`；Codex 最终放行 Stop 后才生成 `completed` 提醒。

Codex 成为前台时：

- 继续清除全部 `completed` 提醒。
- 保留 `waiting`、`failed` 和仍待处理的快捷动作。
- 不把前台切换视为批准动作。

## 测试策略

Rust 集成测试覆盖：

- PermissionRequest 注册、一次批准、fallback、重复解析和超时。
- Stop 标记识别、排除条件、`stop_hook_active` 防循环。
- 多任务并行时动作隔离，长轮询不阻塞普通事件。
- CoPet 退出、token 错误、未知动作和过期动作均不会批准。
- 持久化恢复不会复活可执行动作。
- Hook 输出符合 Codex PermissionRequest 和 Stop 协议。

前端 Playwright 覆盖：

- 普通继续卡片展示内容并调用正确动作命令。
- 工具权限卡片展示命令、目录和“一次性”说明。
- 点击卡片正文仍打开任务，点击动作按钮不会触发打开。
- 不允许快捷处理、过期和需要选择的动作没有批准按钮。
- 双击、命令失败和状态更新具有正确反馈。
- 多张卡片按 `actionId` 独立处理。

## 验收标准

- 普通无歧义任务可从 CoPet 直接继续，同一 Codex 任务恢复运行。
- 工具权限可从 CoPet 只批准当前一次，下一条权限请求仍需再次确认。
- 需要 A/B 选择或参数输入的任务不会出现快捷继续按钮。
- 点击动作按钮不激活 Codex；选择“在 Codex 中处理”才打开对应任务。
- 超时、退出、断连、旧按钮和重复点击均不会误批准。
- completed 前台清除逻辑保持不变，waiting/failed 保留。
- 完整前端测试、Rust 测试、构建和格式检查通过。
