using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows;
using System.Windows.Automation;

namespace TransPop.Services;

public sealed record PointerReport(string Summary, string Details);

/// <summary>
/// A user-triggered accessibility diagnostic. Never injects keys, changes the IME,
/// captures screenshots, or sends text to a network service.
/// Use a background MTA thread for UI Automation calls.
/// </summary>
public static class PointerDiagnostics
{
    private const uint GaRoot = 2;

    public static PointerReport Capture()
    {
        var output = new StringBuilder();
        if (!GetCursorPos(out NativePoint cursor))
            return new PointerReport("无法读取鼠标坐标", "GetCursorPos failed.");

        output.AppendLine($"TransPop UIA diagnostic | Windows build {Environment.OSVersion.Version.Build}");
        output.AppendLine($"Cursor: {cursor.X}, {cursor.Y}");
        IntPtr hwnd = WindowFromPoint(cursor);
        IntPtr root = hwnd == IntPtr.Zero ? IntPtr.Zero : GetAncestor(hwnd, GaRoot);
        AppendWindow(output, "HWND at pointer", hwnd);
        if (root != hwnd)
            AppendWindow(output, "Root HWND", root);
        output.AppendLine();

        try
        {
            var point = new Point(cursor.X, cursor.Y);
            AutomationElement? element = AutomationElement.FromPoint(point);
            if (element is null)
                return new PointerReport("指针下没有 UI Automation 元素", output.ToString());

            output.AppendLine("UIA element and ancestors (Raw View):");
            var walker = TreeWalker.RawViewWalker;
            AutomationElement? current = element;
            AutomationElement? candidateRoot = null;
            int ancestors = 0;
            while (current is not null && ancestors < 12)
            {
                AppendAutomation(output, $"  [{ancestors}]", current);
                if (Safe(() => current.Current.AutomationId) is "IME_Candidate_Window" or "IME_Prediction_Window")
                    candidateRoot = current;
                current = Try(() => walker.GetParent(current));
                ancestors++;
            }

            // Only inspect a small, user-selected branch; never crawl the entire desktop.
            output.AppendLine();
            output.AppendLine("Elements below pointer target (at most 25):");
            AppendChildren(output, element, 0, 25);

            if (candidateRoot is not null)
            {
                output.AppendLine();
                output.AppendLine("Found official IME candidate AutomationId in ancestor chain.");
                output.AppendLine("Candidate-list children (at most 25):");
                AppendChildren(output, candidateRoot, 0, 25);
                return new PointerReport("已找到标准输入法候选节点，查看诊断明细", output.ToString());
            }

            string name = Safe(() => element.Current.Name) ?? "";
            string className = Safe(() => element.Current.ClassName) ?? "";
            string summary = name.Length > 0
                ? "已找到指针下的控件，但未发现标准候选列表 ID"
                : "指针下控件未公开文字，需要检查窗口类名";
            if (className.Length > 0)
                summary += $"（{className}）";
            return new PointerReport(summary, output.ToString());
        }
        catch (Exception ex) when (ex is COMException or ElementNotAvailableException
                                      or InvalidOperationException or UnauthorizedAccessException)
        {
            output.AppendLine($"UI Automation error: {ex.GetType().Name} - {ex.Message}");
            return new PointerReport("UI Automation 访问失败；仍可查看 HWND 信息", output.ToString());
        }
    }

    private static void AppendWindow(StringBuilder sb, string caption, IntPtr hwnd)
    {
        if (hwnd == IntPtr.Zero)
        {
            sb.AppendLine($"{caption}: no HWND");
            return;
        }
        var buffer = new StringBuilder(256);
        _ = GetClassName(hwnd, buffer, buffer.Capacity);
        _ = GetWindowThreadProcessId(hwnd, out uint pid);
        string process = "unknown";
        try { process = Process.GetProcessById(checked((int)pid)).ProcessName; }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception) { }
        sb.AppendLine($"{caption}: 0x{hwnd.ToInt64():X} class={buffer} pid={pid} process={process}");
    }

    private static void AppendAutomation(StringBuilder sb, string prefix, AutomationElement element)
    {
        string name = Safe(() => element.Current.Name) ?? "";
        if (name.Length > 100) name = name[..100] + "…";
        string id = Safe(() => element.Current.AutomationId) ?? "?";
        string cls = Safe(() => element.Current.ClassName) ?? "?";
        string type = Safe(() => element.Current.ControlType.ProgrammaticName) ?? "?";
        int pid = Safe(() => element.Current.ProcessId);
        Rect bounds = Safe(() => element.Current.BoundingRectangle);
        sb.AppendLine($"{prefix} type={type} class={cls} id={id} pid={pid} bounds={bounds}");
        sb.AppendLine($"       name={name}");
    }

    private static void AppendChildren(StringBuilder sb, AutomationElement parent, int depth, int limit)
    {
        if (depth > 1 || limit <= 0) return;
        AutomationElementCollection? children = Try(() => parent.FindAll(
            TreeScope.Children, System.Windows.Automation.Condition.TrueCondition));
        if (children is null) return;
        int count = Math.Min(children.Count, limit);
        for (int i = 0; i < count; i++)
        {
            AutomationElement item = children[i];
            AppendAutomation(sb, new string(' ', depth * 2 + 2) + $"child {i}", item);
            if (depth == 0 && i < 5) AppendChildren(sb, item, depth + 1, 4);
        }
        if (children.Count > count) sb.AppendLine($"  ... {children.Count - count} more children hidden");
    }

    private static T? Try<T>(Func<T> read) where T : class
    {
        try { return read(); }
        catch (Exception ex) when (ex is COMException or ElementNotAvailableException
                                   or InvalidOperationException) { return null; }
    }

    private static T Safe<T>(Func<T> read)
    {
        try { return read(); }
        catch (Exception ex) when (ex is COMException or ElementNotAvailableException
                                   or InvalidOperationException) { return default!; }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativePoint { public int X; public int Y; }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetCursorPos(out NativePoint point);

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(NativePoint point);

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr window, StringBuilder className, int maxCount);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
