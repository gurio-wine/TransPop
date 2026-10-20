# 微软拼音候选侦测 · 第一阶段原型

> **注意：这是验证可行性的原型，不是已验证兼容 Windows 11 的成熟插件。** 没有经过 Windows 11 真机运行或编译验证。当前不包含翻译请求、联网、键盘事件拦截或自动输入。

## 实现范围

- 使用 Windows UI Automation 寻找 `AutomationId = IME_Candidate_Window` 的微软拼音候选列表。
- 在 UIA 返回的子元素中读取含中文的第一个候选项 `Name` 属性。
- 将候选项显示在不抢焦点、鼠标穿透的 WPF 置顶悬浮窗口中。
- 窗口随候选矩形移动，候选消失时自动隐藏。
- 支持手动“延时深度扫描”，用于发现扫描过滤器漏掉的 UIA 窗口。
- 不替换原生输入法，不写入文字，不读取剪贴板，不截图，不联网。

## 运行方法 A：在 Windows 11 上本地运行

1. 安装 [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0)（Windows x64）或 Visual Studio 2026 的 .NET 桌面开发组件。
2. 在本目录打开 PowerShell，执行：

   ```powershell
   dotnet run --project .\PinyinCandidateProbe\PinyinCandidateProbe.csproj
   ```

3. 打开记事本，切换到 **中文（简体）— 微软拼音**。
4. 输入 `nihao`，不要按空格，让中文候选栏保持显示。
5. 如果可读取，显示第一候选和浮窗；否则点击主界面的 **延时深度扫描**，随后在 4 秒内切回记事本并重新输入拼音。

`Ctrl+C` 不一定能关闭 GUI 程序：直接关闭“微软拼音候选侦测”主窗口即可退出。

## 运行方法 B：让 GitHub Actions 打包

将**整个目录里的文件**上传到自己的 GitHub 仓库（包括 `.github/workflows/windows-build.yml`）。

到 **Actions → Build Windows prototype → Run workflow**，等待 Windows runner 构建结束。下载名为 `PinyinCandidateProbe-win-x64` 的构建产物，解压内层 zip，运行 `PinyinCandidateProbe.exe`。

生成的是便携程序，不会注册输入法，没有管理员权限要求。程序仅在运行时显示提示浮窗。

## 为什么不保证一定读得到？

微软公开的 IME 无障碍规范要求候选列表暴露 `IME_Candidate_Window`，子项的 `Name` 应为候选文字：
https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements

但是微软拼音的不同 Win11 版本、候选窗形态或应用场景**不保证**一定呈现这一 UIA 树：

- 自动扫描只检查窗口名称/类名/进程名称像输入法的顶层窗口。如果找到不到，请尝试手动“延时深度扫描”。
- 手动深度扫描仍读不到，可能是候选列表没有公开 UIA 节点，也可能没有以 `IME_Candidate_Window` ID 出现在顶层可见窗口树中。
- UIA 的子节点顺序不一定严格等于候选显示顺序；“第一项”需要真机对照检验。
- 某些显示缩放设置、多显示器及受管理员权限保护的程序中，浮窗可能定位有偏差或无法读取。
- 延时深度扫描较耗时：它遍历多个窗口的 UIA 子树，可能造成短暂 CPU/响应延迟。

若遇到失败，优先反馈 Windows 11 版本、是否使用新/旧版微软拼音、普通扫描和深度扫描的状态文字，以及是否只出现悬浮窗定位偏移。**不要上传正在输入的私人文本。**

## 后续第二阶段（本项目尚未实现）

仅在第一阶段能稳定取得候选后，再新增：翻译引擎（Azure AI Translator / 自定义兼容 AI）、请求去抖、取消过期请求、缓存、配置持久化。翻译只展示，绝不写入文本框。
