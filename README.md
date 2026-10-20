# TransPop（翻译气泡）v0.3｜Windows 10/11 输入法窗口诊断

本次针对 Windows 10 Build 19045 实测线索修复检测方法：`WindowFromPoint` 返回了 `TextInputHost` 的 `Windows.UI.Core.CoreWindow`，但 `AutomationElement.FromPoint` 返回背后的记事本。这说明两者命中的是不同层级，**并不能证明候选已经可以被读取**。

v0.3 新增对鼠标下原生 HWND 的 `AutomationElement.FromHandle` **定向读取**，以及有限层级的 Raw View 子元素遍历、窗口矩形与原生子窗口枚举。这些额外诊断仅查询窗口，不修改任何输入法、也不捕获/发送按键、不截图、不联网。

## 运行方式

GitHub Actions：把压缩包内文件放到仓库根目录，进入 **Actions → Build TransPop Windows diagnostic → Run workflow**，在成功的工作流运行页面下载 `TransPop-win-x64` 工件，解压运行 `TransPop.exe`。

本机：安装 .NET 10 SDK，运行 `dotnet run --project ./TransPop/TransPop.csproj`。

## 复现 Windows 10/11 的候选读取

1. 开启微软拼音，在记事本输入 `nihao`（不要按空格或回车）。
2. 在 TransPop 里点击“5 秒后鼠标定位检测”，切回记事本并重新打开候选框。
3. 把鼠标指针移到**可见的第一条候选词**上，并保持候选列表可见，直到倒计时结束。
4. 返回 TransPop，复制诊断详情，尤其是 `1) UI Automation FromPoint` 与 `2) UI Automation FromHandle` 两段；分享前请检查其中是否有隐私文本。

### 怎么解读

- **FromHandle 里有 `IME_Candidate_Window` / 汉字**：有希望直接以 UIA 获取第一候选，下一步做稳定监听和气泡定位。
- **FromHandle 仅有无名 `Pane` / `Window`**：该版本宿主窗口可能没有暴露可用的候选内容；下一步验证 Windows Accessibility/MSAA 或尝试显式启用的候选框局部 OCR。不能凭这条报告断言所有 Windows 10/11 都不支持。
- **FromPoint 是记事本，FromHandle 是 TextInputHost**：证实两个 API 的命中结果不同。原版基于 FromPoint/顶层窗口的逻辑需要改造。

## 状态与兼容性

- 保持 Windows 10 / Windows 11 共用代码路径；不依赖 Windows 11 独占 API。
- 尚未在此环境的 Windows/.NET 工具链下编译测试；最终效果取决于实际微软拼音版本。
- 不修改候选列表、不自动上屏；Bing 和自定义 AI 还未接入。

参考：https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.automationelement.fromhandle 以及 https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements
