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

- 托盘图标：左键暂停 / 恢复，右键菜单可打开设置、暂停或退出
- 快捷键：`Ctrl+Alt+Q` 退出
- 命令行：`--settings`、`--pause`、`--resume`、`--quit`

## 配置

配置文件：`%LOCALAPPDATA%\TransPop\translator.conf`（首次运行自动生成，默认 `provider=off`，不联网）。托盘「设置...」修改后立即生效。

| 键 | 说明 |
|---|---|
| `provider` | `off` / `bing`（无需密钥）/ `azure` / `openai`（兼容 Chat Completions 接口） |
| `timeout_seconds` | 请求超时，2–30 秒 |
| `debounce_ms` | 翻译防抖，100–1000 毫秒 |
| `alt_output` | `true` 时启用轻按左 Alt 直接输入英文 |

也可以用环境变量覆盖（仅运行时生效，不会写回文件）：`TRANSPOP_PROVIDER`、`TRANSPOP_AZURE_KEY`、`TRANSPOP_AZURE_URL`、`TRANSPOP_AZURE_REGION`、`TRANSPOP_OPENAI_KEY`、`TRANSPOP_OPENAI_URL`、`TRANSPOP_OPENAI_MODEL`。

> API Key 保存时使用 Windows DPAPI 加密（`dpapi:` 前缀，仅当前 Windows 用户可解密）；旧版遗留的明文密钥会在启动时自动重新加密。公共机器上仍建议改用环境变量。

## 日志

`%LOCALAPPDATA%\TransPop\transpop-v<版本>.log`，超过 2 MiB 轮转一次，保留上一份归档。不记录候选文字。

## 已知限制

- `bing` 使用微软 Edge 的非公开网页端点，可能随时失效或限流
- 仅支持微软拼音的候选窗（UIAutomation `IME_Candidate_Window` / `IME_Prediction_Window`）
- API Key 由 DPAPI 绑定当前 Windows 用户，更换用户或重装系统后需要重新填写
- exe 未嵌入版本资源，资源管理器属性页不显示版本号
- 暂停状态、气泡位置不跨重启保存
