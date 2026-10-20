# TransPop · 翻译气泡 v0.4 · 微软拼音自动候选检测

**目标平台：Windows 10 / Windows 11。无需修改系统输入法、无需鼠标指向第一候选。**

这是一版 **读取能力验证原型**：当你在微软拼音输入 `nihao` 并看到候选时，后台自动枚举 `TextInputHost` / `InputApp` / `ctfmon` 的原生窗口，以 `AutomationElement.FromHandle(HWND)` 读取 `IME_Prediction_Window` 或 `IME_Candidate_Window` 的**第一个中文候选**（例如“你好”），然后在候选栏下方自动展示一个圆角气泡。**暂不连接 Bing / AI；气泡目前显示的是中文候选，而不是译文**。

## 在 GitHub Actions 打包（不用安装 .NET）

1. 在 GitHub 创建仓库 `TransPop`，把本 ZIP 解压后的**全部文件和文件夹**上传到仓库根目录；必须保留隐藏的 `.github/workflows/windows-build.yml`。
2. 打开 **Actions → Build TransPop Windows auto-candidate → Run workflow**（推送到 main/master 也会自动构建）。
3. 构建完成后进入该工作流页面，下载名为 `TransPop-win-x64` 的 Artifact（解开 Artifact 内部的 ZIP）。
4. 运行 `TransPop.exe`。发布的是 self-contained win-x64 单文件版本，正常不需要再单独安装 .NET。

本地也可用 .NET 10 SDK：`dotnet run --project ./TransPop/TransPop.csproj`。

## 测试（鼠标放哪里都行）

1. 启动 TransPop，窗口状态显示“等待微软拼音候选框”。
2. 切换到**记事本**，用 Windows 自带的**微软拼音**输入 `nihao`，不要按空格、回车或点击候选词。
3. 观察候选框**下方**是否出现 TransPop 小气泡，显示 **你好**；原程序的状态界面会显示“第一候选：你好”。
4. 继续输入或按 Esc 取消候选，气泡应自动跟随或隐藏。
5. 如果没有出现，回到 TransPop 点击**复制诊断**，将状态日志发来。日志含候选文字，分享前请确认不包含隐私信息。

不用用鼠标指出第一候选，也不用长按快捷键。

## 工作方式与兼容性

- 每 500 毫秒枚举原生 HWND，**只**对微软输入法宿主进程建立 UIA 查询。不截屏、不用 OCR、不读取剪贴板、不监听键盘、不往文本框上屏。
- 同时匹配 `IME_Prediction_Window`（用户 Win10 Build 19045 实测）与 `IME_Candidate_Window`；优先查询候选 `ListItem.Name`，必要时用 Raw View 树遍历兜底。
- 使用自带每显示器 DPI 感知清单，减少非 100% 缩放带来的浮窗偏移问题；不同显示器组合仍需实测。
- WPF 气泡置顶、鼠标穿透、不抢输入焦点。停止运行只需要关闭 TransPop；**不注册 Windows 输入法**，卸载只需删除文件。
- 兼容目标为 Windows 10 22H2 / Windows 11 x64；目前**只有 Windows 10 19045 的手动 HWND 定向读取**经过用户实测验证。无鼠标自动扫描、气泡跟随和 Windows 11 兼容性**尚待实机验证**。
- 如果没有输入法宿主窗口或读不到中文候选，只显示诊断信息，不冒险读取普通输入框文字。

## 后续

确认自动读取 + 贴边气泡都正常后，再增加 Bing/Azure Translator 和 OpenAI 兼容 API、自动防抖缓存、设置界面和系统托盘。
