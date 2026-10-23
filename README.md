# TransPop

在 Windows 上用微软拼音打字时，把**当前高亮的候选**实时翻译，并贴在输入法候选窗下方弹出——不抢焦点、可点击穿透。中文候选翻成英文；直接输入英文等其它语言时，反过来翻成中文。可选「轻按一个快捷键（默认 Tab）把翻译上屏」（实验性，默认关闭）。

## 环境要求

- Windows 10 / 11
- Rust 稳定版 + MSVC 工具链（`rustup default stable-msvc`）
- 可选：使用 `azure` / `openai` 翻译时需要对应 API Key

## 构建与运行

```powershell
cargo build --release
.\target\release\transpop.exe
```

`build.ps1` 是本机安装脚本（不在仓库内）：构建成功后关闭旧实例并把 exe 覆盖到固定目录。

## 使用

- 托盘图标：左键暂停 / 恢复，右键菜单可打开设置、检查更新、暂停或退出
- 设置窗口分五个标签：翻译 / 外观 / 系统 / 快捷键 / 关于，改动即改即存，没有保存按钮，关掉窗口也不会丢
- 同类控件共用一套样式预设：统一高度、统一圆角，不用边框，只靠底色区分
- 输入框和下拉框都是圆角；下拉展开的选项列表与窗口内控件同一套配色，没有勾选栏和聚焦描边
- 上屏是即时注入：Escape 丢弃拼音组合与译文在同一次输入里送入，不经过剪贴板，不污染剪贴板历史
- 快捷键：`Ctrl+Alt+Q` 退出
- 命令行：`--settings`、`--pause`、`--resume`、`--quit`

## 配置

配置文件：`%LOCALAPPDATA%\TransPop\translator.conf`（首次运行自动生成，默认 `provider=off`，不联网）。托盘「设置...」或 `--settings` 打开设置窗口，改动即刻写入并立即生效。

| 键 | 说明 |
|---|---|
| `provider` | `off` / `bing`（无需密钥）/ `azure` / `openai`（兼容 Chat Completions 接口） |
| `timeout_seconds` | 请求超时，2–30 秒 |
| `debounce_ms` | 翻译防抖，100–1000 毫秒 |
| `alt_output_shortcut` | 翻译上屏的快捷键，默认 `tab`；可写 `ctrl+shift+k` 这类组合键，写 `none` 关闭；可设多个，用逗号分隔（最多 3 个） |

| `auto_install` | `on`（默认）时发现新版本直接后台下载安装，`off` 时只弹窗询问 |
| `auto_restart` | `on`（默认）时安装完自动重启到新版本；`off` 时只替换文件，下次启动生效 |
| `update_interval_minutes` | 后台检查间隔，单位分钟，最小 15，默认 30；写 `0` 表示仅手动检查 |

设置窗口「快捷键」页的每个框点一下就会清空并显示「请设置按键」，接着按下的键即为快捷键，录入后自动失焦，要改必须再点一次。点「+」新增一个框；点窗口空白处清空该框，清空的框会被删掉，已设的快捷键不会少于一个。

也可以用环境变量覆盖（仅运行时生效，不会写回文件）：`TRANSPOP_PROVIDER`、`TRANSPOP_AZURE_KEY`、`TRANSPOP_AZURE_URL`、`TRANSPOP_AZURE_REGION`、`TRANSPOP_OPENAI_KEY`、`TRANSPOP_OPENAI_URL`、`TRANSPOP_OPENAI_MODEL`、`TRANSPOP_AUTO_INSTALL`、`TRANSPOP_AUTO_RESTART`、`TRANSPOP_UPDATE_INTERVAL_MINUTES`。旧版的 `TRANSPOP_AUTO_UPDATE=off` 仍被识别，等同于「仅手动」。

> API Key 保存时使用 Windows DPAPI 加密（`dpapi:` 前缀，仅当前 Windows 用户可解密）；旧版遗留的明文密钥会在启动时自动重新加密。公共机器上仍建议改用环境变量。

暂停状态保存在 `%LOCALAPPDATA%\TransPop\state.conf`，重启后自动恢复。

## 自动更新

启动 20 秒后以及之后每隔 `update_interval_minutes`（默认 30 分钟，可在设置窗口选择 15 分钟到 1 周，或选「仅手动」完全关闭定时检查），TransPop 会查询 GitHub Releases 的最新版本。`auto_install` 决定查到之后怎么办：`on`（默认）时全程后台完成，不弹窗、不抢焦点，下载后用 GitHub 公布的 `sha256` 摘要校验，通过后替换自身，并按 `auto_restart` 决定是否立刻重启到新版本（新进程会等旧进程退出后再启动；关闭时下次启动生效）；`off` 时只弹出前台窗口告知版本并询问，确认后才下载，下载过程在同一窗口显示进度。设置窗口「关于」页的「检查更新」始终走询问流程。

无法直连 github.com 时，更新检查与下载会自动改用 GitHub 反向代理（`ghfast.top`、`gh.llkk.cc`、`ghproxy.net`）。镜像模式下必须有两个相互独立的镜像公布同一版本号与同一 `sha256`，才会下载并安装，避免单个代理投毒；仍无法更新时可用环境变量 `TRANSPOP_UPDATE_MIRROR=https://你的代理` 指定可信代理。选择「稍后」则本次不下载，下次检查或手动检查时再询问。托盘右键「检查更新...」或设置窗口「关于」页的「检查更新」可手动触发同一流程。更新文件临时存放在 `%LOCALAPPDATA%\TransPop\update\`，安装或下次启动时清空。

更新时旧版本会先改名为 `transpop.exe.old`。新版本成功进入消息循环并稳定运行 30 秒后才删除该备份；若新版本启动即崩溃，旧的 `transpop.exe.old` 会保留，可手动改回 `transpop.exe` 回滚。

## 日志

日志统一放在 `%LOCALAPPDATA%\TransPop\logs\`：当前日志为 `transpop.log`，跨天后自动归档为 `transpop-<年月日>.log`（如 `transpop-20261023.log`），超过 7 天的归档由程序自动删除。不记录候选文字。

## 已知限制

- `bing` 使用微软 Edge 的非公开网页端点，可能随时失效或限流
- 仅支持微软拼音的候选窗（UIAutomation `IME_Candidate_Window` / `IME_Prediction_Window`）
- API Key 由 DPAPI 绑定当前 Windows 用户，更换用户或重装系统后需要重新填写
- 气泡位置跟随输入法候选窗计算，不支持拖动或跨重启记住位置
- 自动更新优先直连 GitHub；连不上时依赖第三方反向代理，若这些代理也失效需要手动更新，或用 `TRANSPOP_UPDATE_MIRROR` 指定代理。安装目录不可写（例如 Program Files）时替换会失败并弹窗提示，同时写入日志
- 回滚备份 `transpop.exe.old` 只保留到新版本稳定运行 30 秒；更早的崩溃需要重新安装旧版本
