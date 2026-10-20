using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows;
using System.Windows.Automation;

namespace TransPop.Services;

public sealed record PointerReport(string Summary, string Details);

/// <summary>
/// Diagnostic only: read the native HWND under the pointer and compare it to UIA's
/// ElementFromPoint, which can return the application *behind* the IME popup.
/// Never injects input, changes candidate selection, takes screenshots or sends data.
/// Run from a background MTA thread (see MainWindow).
/// </summary>
public static class PointerDiagnostics
{
    private const uint GaRoot = 2;
    private const int MaxRawNodes = 55;

    public static PointerReport Capture()
    {
        var log = new StringBuilder();
        if (!GetCursorPos(out NativePoint cursor))
            return new PointerReport("无法读取鼠标坐标", "GetCursorPos failed.");

        log.AppendLine($"TransPop v0.3 native HWND/UIA diagnostic | Windows build {Environment.OSVersion.Version.Build}");
        log.AppendLine($"Cursor: {cursor.X}, {cursor.Y}");
        IntPtr hwnd = WindowFromPoint(cursor);
        IntPtr root = hwnd == IntPtr.Zero ? IntPtr.Zero : GetAncestor(hwnd, GaRoot);
        AppendWindow(log, "Native HWND under cursor", hwnd);
        if (root != hwnd)
            AppendWindow(log, "Root native HWND", root);

        log.AppendLine();
        log.AppendLine("1) UI Automation FromPoint (may hit the editor behind the IME)");
        var fromPoint = SafeRead(() => AutomationElement.FromPoint(new Point(cursor.X, cursor.Y)));
        if (fromPoint is null)
        {
            log.AppendLine("  FromPoint returned null or threw an exception.");
        }
        else
        {
            AppendAutomation(log, "  [0]", fromPoint);
            var walker = TreeWalker.RawViewWalker;
            var current = fromPoint;
            for (var i = 1; i <= 5 && current is not null; i++)
            {
                current = SafeRead(() => walker.GetParent(current));
                if (current is not null) AppendAutomation(log, $"  parent {i}", current);
            }
        }

        log.AppendLine();
        log.AppendLine("2) UI Automation FromHandle (directly inspect the native HWND)");
        var target = hwnd == IntPtr.Zero ? null : SafeRead(() => AutomationElement.FromHandle(hwnd));
        bool foundImeId = false;
        bool foundChinese = false;
        if (target is null)
        {
            log.AppendLine("  FromHandle returned null or threw an exception.");
        }
        else
        {
            AppendAutomation(log, "  handle root", target);
            ExploreRawTree(log, target, ref foundImeId, ref foundChinese);
        }

        log.AppendLine();
        log.AppendLine("3) Native child HWNDs of the window under the pointer (first 18)");
        if (hwnd == IntPtr.Zero)
        {
            log.AppendLine("  No HWND.");
        }
        else
        {
            var seen = 0;
            // EnumChildWindows walks native child HWNDs, not just UIA nodes.
            EnumChildWindows(hwnd, (child, _) =>
            {
                if (seen++ >= 18) return false;
                AppendWindow(log, $"  child {seen}", child);
                return true;
            }, IntPtr.Zero);
            if (seen == 0) log.AppendLine("  No native child windows.");
            else if (seen > 18) log.AppendLine("  ... additional native child windows omitted");
        }

        string nativeProcess = GetProcessName(hwnd);
        var seemsTextHost = nativeProcess.Contains("TextInputHost", StringComparison.OrdinalIgnoreCase)
                            || nativeProcess.Contains("InputApp", StringComparison.OrdinalIgnoreCase);

        string summary = foundImeId
            ? "已找到 IME_Candidate_Window；候选树可进一步解析"
            : foundChinese
                ? "直接读取 HWND 的 UIA 树看到了中文，请检查候选节点"
                : seemsTextHost
                    ? "命中 TextInputHost，但从该 HWND 还没读到候选文字"
                    : "未确认输入法窗口；请把鼠标停在可见的第一候选上";
        return new PointerReport(summary, log.ToString());
    }

    private static void ExploreRawTree(StringBuilder log, AutomationElement root,
        ref bool foundImeId, ref bool foundChinese)
    {
        // Limited, breadth-first read of the subtree of the HWND, not the desktop.
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Element, int Depth)>();
        queue.Enqueue((root, 0));
        var visited = 0;
        while (queue.Count > 0 && visited < MaxRawNodes)
        {
            var (element, depth) = queue.Dequeue();
            visited++;
            if (depth > 0) AppendAutomation(log, $"  node {visited} depth={depth}", element);

            string id = SafeRead(() => element.Current.AutomationId) ?? "";
            string name = SafeRead(() => element.Current.Name) ?? "";
            if (id is "IME_Candidate_Window" or "IME_Prediction_Window") foundImeId = true;
            if (ContainsChinese(name)) foundChinese = true;
            if (depth >= 4) continue;

            var child = SafeRead(() => walker.GetFirstChild(element));
            int siblingCount = 0;
            while (child is not null && siblingCount++ < 24 && queue.Count < MaxRawNodes)
            {
                queue.Enqueue((child, depth + 1));
                var previous = child;
                child = SafeRead(() => walker.GetNextSibling(previous));
            }
        }
        log.AppendLine($"  traversed {visited} UIA node(s); " +
            (queue.Count > 0 ? "node limit reached." : "subtree traversal complete."));
        log.AppendLine($"  Candidate AutomationId found={foundImeId}; Chinese text in UIA Name={foundChinese}");
    }

    private static bool ContainsChinese(string value)
    {
        foreach (char ch in value)
            if (ch >= '\u3400' && ch <= '\u9FFF') return true;
        return false;
    }

    private static string GetProcessName(IntPtr hwnd)
    {
        if (hwnd == IntPtr.Zero) return "";
        _ = GetWindowThreadProcessId(hwnd, out uint pid);
        try { using var p = Process.GetProcessById(checked((int)pid)); return p.ProcessName; }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception) { return "unknown"; }
    }

    private static void AppendWindow(StringBuilder sb, string caption, IntPtr hwnd)
    {
        if (hwnd == IntPtr.Zero) { sb.AppendLine($"{caption}: none"); return; }
        var buffer = new StringBuilder(256);
        _ = GetClassName(hwnd, buffer, buffer.Capacity);
        _ = GetWindowThreadProcessId(hwnd, out uint pid);
        string rectangle = GetWindowRect(hwnd, out NativeRect rect)
            ? $"({rect.Left},{rect.Top})-({rect.Right},{rect.Bottom})" : "unavailable";
        sb.AppendLine($"{caption}: 0x{hwnd.ToInt64():X} class={buffer} pid={pid} " +
                      $"process={GetProcessName(hwnd)} visible={IsWindowVisible(hwnd)} rect={rectangle}");
    }

    private static void AppendAutomation(StringBuilder sb, string caption, AutomationElement element)
    {
        string name = SafeRead(() => element.Current.Name) ?? "";
        if (name.Length > 90) name = name[..90] + "…";
        var id = SafeRead(() => element.Current.AutomationId) ?? "?";
        var cls = SafeRead(() => element.Current.ClassName) ?? "?";
        var type = SafeRead(() => element.Current.ControlType.ProgrammaticName) ?? "?";
        int pid = SafeRead(() => element.Current.ProcessId);
        Rect bounds = SafeRead(() => element.Current.BoundingRectangle);
        sb.AppendLine($"{caption}: type={type} class={cls} id={id} pid={pid} bounds={bounds}");
        sb.AppendLine($"    name={name}");
    }

    private static T SafeRead<T>(Func<T> action)
    {
        try { return action(); }
        catch (Exception ex) when (ex is COMException or ElementNotAvailableException
                                   or InvalidOperationException or UnauthorizedAccessException) { return default!; }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativePoint { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeRect { public int Left; public int Top; public int Right; public int Bottom; }

    private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lparam);

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

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetWindowRect(IntPtr window, out NativeRect rect);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lparam);
}
