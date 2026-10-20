# TransPop（翻译气泡）v0.5 · Windows 10/11 自动候选修复版

**这一版仍是候选检测原型，不会请求网络翻译，不会把候选上屏。** 目前气泡只显示微软拼音的第一个中文候选，用于验证自动发现是否稳定。

## 修复内容

v0.4 在你的 Windows 10 上一直显示“等待输入”。与之前 v0.3 鼠标定向诊断成功的结果对照，我们发现 v0.4 的自动发现逻辑过于宽泛：它先枚举 `TextInputHost/InputApp/ctfmon` 的大量窗口，并对它们执行 UI Automation 子树查询，没有优先定位 v0.3 已验证的窗口类型。这有可能导致遗漏或查询过慢，但**未获取 v0.4 的完整诊断日志前，无法百分之百确定具体原因**。

v0.5 做了如下修订：

1. 原生 `EnumWindows` + `EnumChildWindows` 枚举桌面主窗口及其**所有原生子窗口**，不需要鼠标位置。
2. 优先检查 Win10 19045 实测的 `Windows.UI.Core.CoreWindow`，包括位于 Explorer `ApplicationFrameWindow` 下的宿主子窗口。其他 `TextInputHost/InputApp` 窗口作为兼容回退。
3. `AutomationElement.FromHandle(hwnd)` 直接读取 UIA **Raw View**，查找 `IME_Prediction_Window` / `IME_Candidate_Window`，读取其第一个 `ListItem.Name`。
4. 不再先对不相关窗口运行深层 `FindFirst(TreeScope.Descendants)`；最多检查 24 个候选目标窗口，优先处理最像候选框的短窗口。
5. 把 `CoreWindow`、宿主 HWND、检查数量、面板命中数以及 UIA 异常写入程序日志。如果超过 3 秒无结果，状态会显示“UI Automation 查询超过 3 秒”。
6. 背景扫描约每 350 毫秒执行一次；泡泡继续在候选窗口下方显示，不需要鼠标移动。

## 用 GitHub Actions 构建

上传 ZIP 内的 **`TransPop/` 和 `.github/`** 至你现有 GitHub 仓库根目录，覆盖原来的文件。打开 Actions → **Build TransPop v0.5 Windows auto-candidate** → Run workflow，下载 Artifact 内的 `TransPop-win-x64.zip`，解压运行 `TransPop.exe`。

如果需要本地编译：在安装了 .NET 10 SDK 的 Windows 上，运行 `dotnet run --project TransPop/TransPop.csproj`。

## 实测方式

1. 退出旧版 TransPop.exe，启动 v0.5。
2. 打开记事本、切到微软拼音，输入 `nihao`，不要按空格，让候选框保持展开。
3. 稍等，程序界面应显示“第一候选：你好”，气泡也应出现。**不需要移动鼠标。**
4. 如果仍然等待，请回到 TransPop，点击“复制诊断”，把日志发过来。特别留意类似这行：

   `CoreWindow=1 / 宿主HWND=2 / UIA检查=1` 

   - CoreWindow=0：没有枚举到已验证类型的原生窗口，重点检查系统宿主窗口或权限。
   - CoreWindow>0 但候选面板=0：已找到窗口，Raw View 中没有读到候选面板。
   - 候选面板>0 但未读到中文：候选项访问、可见性、元素变化需要进一步分析。
   - UIA 查询超过 3 秒：可能有 Windows UIA 提供程序响应缓慢。

诊断日志可能包含正在输入的候选词。分享前请确认其中没有敏感文字。

## 当前验证状态

代码与配置文件已做静态检查；无法在这个环境直接运行 Windows UIA，因此本版**尚未完成 Win10/Win11 实机运行确认**。目前已实测成功的仍是你在 Win10 19045 使用 v0.3 从 HWND 读取第一候选。Windows 11 自动识别仍待测试。
