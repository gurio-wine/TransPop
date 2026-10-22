# TransPop

在 Windows 上用微软拼音打字时，把**当前高亮的中文候选**实时翻译成英文，并贴在输入法候选窗下方弹出——不抢焦点、可点击穿透。可选「轻按左 Alt 取消拼音并直接输入英文」（实验性，默认关闭）。

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
- 设置窗口标题旁显示当前版本；窗口内可直接开关「自动检查 GitHub 更新」，也可点「检查更新」
- 快捷键：`Ctrl+Alt+Q` 退出
- 命令行：`--settings`、`--pause`、`--resume`、`--quit`

## 配置

配置文件：`%LOCALAPPDATA%\TransPop\translator.conf`（首次运行自动生成，默认 `provider=off`，不联网）。托盘「设置...」或 `--settings` 打开设置窗口，保存后立即生效。

| 键 | 说明 |
|---|---|
| `provider` | `off` / `bing`（无需密钥）/ `azure` / `openai`（兼容 Chat Completions 接口） |
| `timeout_seconds` | 请求超时，2–30 秒 |
| `debounce_ms` | 翻译防抖，100–1000 毫秒 |
| `alt_output` | `true` 时启用轻按左 Alt 直接输入英文 |
| `auto_update` | `on`（默认）时自动检查 GitHub 新版本并弹窗询问是否更新，`off` 关闭 |

也可以用环境变量覆盖（仅运行时生效，不会写回文件）：`TRANSPOP_PROVIDER`、`TRANSPOP_AZURE_KEY`、`TRANSPOP_AZURE_URL`、`TRANSPOP_AZURE_REGION`、`TRANSPOP_OPENAI_KEY`、`TRANSPOP_OPENAI_URL`、`TRANSPOP_OPENAI_MODEL`、`TRANSPOP_AUTO_UPDATE`。

> API Key 保存时使用 Windows DPAPI 加密（`dpapi:` 前缀，仅当前 Windows 用户可解密）；旧版遗留的明文密钥会在启动时自动重新加密。公共机器上仍建议改用环境变量。

暂停状态保存在 `%LOCALAPPDATA%\TransPop\state.conf`，重启后自动恢复。

## 自动更新

启动 20 秒后以及之后每 6 小时，TransPop 会查询 GitHub Releases 的最新版本（`auto_update=on`，默认，可在设置窗口关闭）。发现新版本时先弹出前台窗口告知版本并询问是否更新，确认后才开始下载，下载过程在同一窗口显示进度；下载完成后用 GitHub 公布的 `sha256` 摘要校验，通过后替换自身并自动重启（新进程会等旧进程退出后再启动）。选择「稍后」则本次不下载，下次检查或手动检查时再询问。托盘右键「检查更新...」或设置窗口的「检查更新」可手动触发同一流程。更新文件存放在 `%LOCALAPPDATA%\TransPop\update\`。

更新时旧版本会先改名为 `transpop.exe.old`。新版本成功进入消息循环并稳定运行 30 秒后才删除该备份；若新版本启动即崩溃，旧的 `transpop.exe.old` 会保留，可手动改回 `transpop.exe` 回滚。

## 日志

`%LOCALAPPDATA%\TransPop\transpop-v<版本>.log`，超过 2 MiB 轮转一次，保留上一份归档。不记录候选文字。

## 已知限制

- `bing` 使用微软 Edge 的非公开网页端点，可能随时失效或限流
- 仅支持微软拼音的候选窗（UIAutomation `IME_Candidate_Window` / `IME_Prediction_Window`）
- API Key 由 DPAPI 绑定当前 Windows 用户，更换用户或重装系统后需要重新填写
- 气泡位置跟随输入法候选窗计算，不支持拖动或跨重启记住位置
- 自动更新需要能访问 GitHub；若安装目录不可写（例如 Program Files），替换会失败并弹窗提示，同时写入日志
- 回滚备份 `transpop.exe.old` 只保留到新版本稳定运行 30 秒；更早的崩溃需要重新安装旧版本
