<div align="center">
  <img src="./public/pet.png" alt="CoPet logo" width="120" />
  <h1>CoPet</h1>
  <p><strong>给 Codex 配一只安静常驻的桌面宠物。</strong></p>
  <p>CoPet 使用兼容 Codex 的宠物包，以静态形象常驻桌面，并把任务完成、等待与失败状态变成可操作提醒。</p>
</div>

![CoPet](./public/banner.zh.png)

[English](./README.md)

基于 Tauri、Rust 和 React 构建。轻量、本地优先，不依赖云服务。

## 内置宠物

<table>
  <tr>
    <td align="center"><img src="./public/pets/copet-neo.gif" width="96" alt="CoPet Neo"><br><sub>CoPet Neo</sub></td>
    <td align="center"><img src="./public/pets/copet-nia.gif" width="96" alt="CoPet Nia"><br><sub>CoPet Nia</sub></td>
    <td align="center"><img src="./public/pets/copet-mecha.gif" width="96" alt="CoPet Mecha"><br><sub>CoPet Mecha</sub></td>
    <td align="center"><img src="./public/pets/dj-fuzz.gif" width="96" alt="DJ Fuzz"><br><sub>DJ Fuzz</sub></td>
    <td align="center"><img src="./public/pets/dog.gif" width="96" alt="Lucky Dog"><br><sub>Lucky Dog</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="./public/pets/dragon.gif" width="96" alt="Azure Dragon"><br><sub>Azure Dragon</sub></td>
    <td align="center"><img src="./public/pets/duck.gif" width="96" alt="Waddly Duck"><br><sub>Waddly Duck</sub></td>
    <td align="center"><img src="./public/pets/goat.gif" width="96" alt="Cloud Goat"><br><sub>Cloud Goat</sub></td>
    <td align="center"><img src="./public/pets/goku.gif" width="96" alt="Goku"><br><sub>Goku</sub></td>
    <td align="center"><img src="./public/pets/horse.gif" width="96" alt="Chestnut Horse"><br><sub>Chestnut Horse</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="./public/pets/monkey.gif" width="96" alt="Clever Monkey"><br><sub>Clever Monkey</sub></td>
    <td align="center"><img src="./public/pets/orange-cat.gif" width="96" alt="Orange Cat"><br><sub>Orange Cat</sub></td>
    <td align="center"><img src="./public/pets/ox.gif" width="96" alt="Cream Ox"><br><sub>Cream Ox</sub></td>
    <td align="center"><img src="./public/pets/panda.gif" width="96" alt="Panda"><br><sub>Panda</sub></td>
    <td align="center"><img src="./public/pets/pig.gif" width="96" alt="Blush Pig"><br><sub>Blush Pig</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="./public/pets/rabbit.gif" width="96" alt="White Rabbit"><br><sub>White Rabbit</sub></td>
    <td align="center"><img src="./public/pets/rat.gif" width="96" alt="Pearl Rat"><br><sub>Pearl Rat</sub></td>
    <td align="center"><img src="./public/pets/rooster.gif" width="96" alt="Golden Rooster"><br><sub>Golden Rooster</sub></td>
    <td align="center"><img src="./public/pets/snake.gif" width="96" alt="Jade Snake"><br><sub>Jade Snake</sub></td>
    <td align="center"><img src="./public/pets/tiger.gif" width="96" alt="Striped Tiger"><br><sub>Striped Tiger</sub></td>
  </tr>
</table>

## 主要功能

- 宠物固定显示静态帧，可拖拽调整位置；短按宠物会打开 Codex，拖拽不会误打开。
- 首版专注支持 Codex，并在首次启动时自动安装可用的 Codex Hook。
- Codex 任务完成或等待操作时显示独立提醒；点击提醒可返回对应任务。
- 在 macOS 上，Codex 成为前台应用后会自动清除全部完成提醒，同时保留等待和失败提醒。
- 自带多款宠物，也可以导入兼容 Codex 的宠物包。
- 支持长按和原生右键菜单。
- 可在设置页和托盘中调整宠物尺寸、Agent 消息显示方式、hooks、语言、显示状态和窗口位置。
- Agent 消息既可以只显示最新一条，也可以同时保留多条更新。
- 数据默认留在本机，存放于 `~/.copet`；hook 写入会先备份、再原子写入，且不包含遥测。

## 安装

| 平台 | 下载 |
| --- | --- |
| macOS（通用版） | [CoPet-macos-universal.dmg](https://github.com/ChanceYu/CoPet/releases/latest/download/CoPet-macos-universal.dmg) |
| Windows x64 | [CoPet-windows-x64.exe](https://github.com/ChanceYu/CoPet/releases/latest/download/CoPet-windows-x64.exe) |

[全部版本](https://github.com/ChanceYu/CoPet/releases)

### macOS

把 `CoPet.app` 拖到 `/Applications`。当前构建未经过公证，首次运行前需要执行一次以下命令清除隔离标记：

```bash
sudo xattr -rd com.apple.quarantine /Applications/CoPet.app
```

安装后可从“应用程序”目录启动 CoPet。若在“设置 → 通用”中开启“登录后自动启动”，关机或退出登录后，下一次登录 macOS 时 CoPet 会自动启动。

### Windows

Windows 版本未进行代码签名。首次启动时如果 SmartScreen 弹出警告，请点击 *更多信息* → *仍要运行*。

## 自定义你的宠物

CoPet 不只能使用内置宠物。[CoPet Skill 系列](./skills/README.md) 可以把角色设定、团队吉祥物或个人头像做成你的桌面伙伴：

- [`copet-gen`](./skills/copet-gen/SKILL.md) 生成并安装自定义 CoPet 宠物包，包含 `pet.json` 与 `spritesheet.webp`，让你的宠物跟随 Agent 活动做出反应。
- [`copet-sound`](./skills/copet-sound/SKILL.md) 生成配套 11 段 MP3 音效包，用于点击、手势、等待、成功和错误等场景。

安装到 Codex，有两种方式。

在终端中运行：

```bash
npx skills add ChanceYu/CoPet --skill '*' -a codex
```

在 Codex 会话中输入：

```text
$skill-installer install all CoPet skills from https://github.com/ChanceYu/CoPet/tree/main/skills
```

如果安装后没有看到这些 Skill，请重启 Codex。

> **仅支持 Codex。** `copet-gen` 依赖上游 `$hatch-pet` / `$imagegen` 完成图像生成，而 `$imagegen` 的默认模式依赖 Codex 自带的 `image_gen` 工具。Claude Code、Cursor 等 Agent 没有该工具，因此不支持。

## Codex 任务提醒

CoPet 首版只在设置页暴露 Codex 集成，配置路径为 `~/.codex/hooks.json` 和 `~/.codex/config.toml`。任务结束后宠物会显示“任务完成啦，快去看看吧。”；短按宠物可直接打开 Codex。在 macOS 上，Codex 成为前台后会清除全部完成提醒，但保留等待和失败提醒。

详细行为、排障和验收步骤见 [Codex 任务提醒说明](./docs/codex-task-reminders.md)。

## 快速开始

需要先安装 [Rust](https://www.rust-lang.org/tools/install)、[Node.js](https://nodejs.org/) 和 pnpm。支持 macOS（主要平台）、Windows 和 Linux。

```bash
git clone https://github.com/ChanceYu/CoPet.git
cd CoPet
pnpm install
pnpm tauri dev          # 开发模式：React 改动热更新，Rust 改动会重编译
pnpm tauri build        # 构建正式安装包
```

调整前端样式和文案时通常无需完整打包，可直接在开发模式预览；修改 Rust 代码或依赖时需要重新编译。正式交付时再运行 `pnpm tauri build`。

## 项目结构

- `src-tauri/` — Rust 核心、Agent 适配器和运行时事件服务器。
- `src/` — React 前端（宠物窗口和设置中心）。
- `src-tauri/assets/pets/` — 随应用打包的内置宠物包。
- `src-tauri/assets/sounds/` — 随应用打包的内置全局音效包。
- `skills/` — 可选的 CoPet Skill 文档，用于生成宠物和 11 段音效包。
- `docs/architecture.zh.md` — 技术架构与设计文档。
- `AGENTS.md` — 贡献者指南与测试说明。

## 安全

- 事件服务器只监听 `127.0.0.1`，请求必须携带 bearer token；服务会限流，并丢弃未知 payload。
- 所有 hook 配置改写都会先备份原始字节，再使用原子写入。
- 宠物包和音效包都会被视为不可信数据，使用前必须校验。
- `assetProtocol.scope` 只允许 webview 读取宠物、音效、预览和内置资源目录。

## 贡献

欢迎提交 Issue 和 PR。开发前请先阅读 [AGENTS.md](AGENTS.md) 了解环境与约定，并阅读 [docs/architecture.zh.md](docs/architecture.zh.md) 了解系统设计。

## 许可证

[MIT](LICENSE) © ChanceYu
