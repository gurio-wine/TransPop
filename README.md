# TransPop · 翻译气泡

Windows 10 / Windows 11 微软拼音候选检测原型（v0.2，尚未接入翻译）。

> 当前目的只有一个：检测微软拼音是否向独立桌面程序暴露候选框、第一候选的文字和坐标。**不修改输入法，不上屏，不录屏，不联网，不申请管理员权限。**
>
> 代码尚未在 Windows 10/11 真机验证；是否能读到候选取决于微软拼音版本及其无障碍接口。

## 使用说明

### 方式 A：GitHub Actions

1. 新建 GitHub 仓库（例如 `TransPop`），将本压缩包**内部**全部文件上传到仓库根目录；不要再额外套一层目录。
2. 进入 Actions → **Build TransPop Windows diagnostic** → Run workflow。
3. 构建成功后下载 `TransPop-win-x64` artifact，解压 ZIP，运行 `TransPop.exe`。

构建命令已在 `.github/workflows/windows-build.yml` 中配置。产物是自包含便携 Windows x64 程序，无须安装 .NET Runtime。GitHub Windows 运行器没有在这次制作环境中实际运行过，故不保证首次 CI 成功。

### 方式 B：Windows 本地开发

安装 [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0)，在项目根目录运行：

```powershell
dotnet run --project .\TransPop\TransPop.csproj
```

## 怎么测试（优先用鼠标定位）

1. 确认在用 **微软拼音**，打开**记事本**。
2. 启动 `TransPop.exe`，单击 **5 秒后鼠标定位检测**。
3. 在 5 秒内切回记事本并输入 `nihao`，**先不要按空格或回车**。
4. **把鼠标移动到候选窗口里的第 1 个汉字候选词上方**，保持候选框展开直到倒计时结束。只移动鼠标，不要点击候选词。
5. 返回 TransPop，查看诊断信息。

如果自动扫描没有显示气泡，**不能推断 Windows 10 不兼容**。本版新增的鼠标定位诊断会同时采集：

- 鼠标下 Win32 窗口的 `HWND`、类名与进程名。
- 鼠标下 UI Automation 元素的 `AutomationId`、`ControlType`、`Name` 与外接矩形。
- 元素的向上父级路径和部分子控件层级。

如果鼠标下能读到汉字但没有 `IME_Candidate_Window`，说明我们之前基于固定 ID 的检测存在漏报，**不意味着系统不给读**。如果连 UIA `Name` 都读不到，也可能需要改用 Win32/MSAA（或者用户主动触发 OCR）路径；这尚未实现。

按钮 **延时深度扫描**仍然存在，但在某些 Windows 10 / 11 输入法候选实现中它可能漏报，优先用**鼠标定位诊断**。

## 分享诊断结果时要注意隐私

报告可能包含光标下控件的文字、汉字候选，以及 Windows 用户的界面信息。点击“复制诊断”之后，**请先删除私人文本、用户名及应用内容**再分享。程序不会上传任何内容。

## 不在本原型范围内

- Bing / Azure Translator、OpenAI 兼容接口（下一阶段）。
- 输入法候选列表注入、复制候选到编辑框、自动上屏。
- 捕获按键、读取剪贴板、全屏 OCR。

技术参考：

- [Microsoft IME accessibility conventions](https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements)
- [UI Automation AutomationElement.FromPoint](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.automationelement.frompoint)
