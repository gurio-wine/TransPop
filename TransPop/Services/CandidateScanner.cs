using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows;
using System.Windows.Automation;

namespace TransPop.Services;

public sealed record CandidateHit(string FirstCandidate, Rect Anchor, string Source);
public sealed record ScanResult(CandidateHit? Hit, int Hosts, int Windows,
    int Inspected, int Panes, string Note);

/// <summary>
/// Windows 10/11 Microsoft Pinyin candidate probe.
/// 1) Enumerate native HWNDs (including children of explorer's ApplicationFrameWindow).
/// 2) Prioritize Windows.UI.Core.CoreWindow, the exact HWND verified on Win10 19045.
/// 3) Read its UI Automation RAW tree using AutomationElement.FromHandle.
/// No cursor location, clipboard, editor contents, keystrokes, OCR or input injection.
/// Must be called from a non-UI MTA thread.
/// </summary>
public sealed class CandidateScanner
{
    private static readonly string[] HostNames = ["TextInputHost", "InputApp"];
    private const int MaxUiaWindows = 24;

    public ScanResult Scan()
    {
        var hostPids = FindHostPids();
        var windows = FindCandidateWindows(hostPids, out int coreCount, out int hostWindowCount);
        int inspected = 0;
        int panes = 0;
        string lastError = "";

        foreach (NativeWindow w in windows.Take(MaxUiaWindows))
        {
            inspected++;
            try
            {
                if (TryRead(w.Handle, out CandidateHit? hit, out bool paneSeen))
                {
                    if (paneSeen) panes++;
                    return new ScanResult(hit, hostPids.Count, windows.Count, inspected, panes,
                        $"找到第一候选 · CoreWindow={coreCount}，宿主窗口={hostWindowCount}");
                }
                if (paneSeen) panes++;
            }
            catch (Exception e) when (e is COMException or ElementNotAvailableException or
                                       InvalidOperationException or UnauthorizedAccessException)
            {
                lastError = e.GetType().Name;
            }
        }

        string diagnostics = $"CoreWindow={coreCount} / 宿主HWND={hostWindowCount} / UIA检查={inspected}";
        string note = panes > 0
            ? "发现候选面板，但尚未读取到可见中文候选"
            : windows.Count == 0
                ? "未找到输入法宿主窗口；请确认微软拼音候选框已展开"
                : "找到输入法窗口，但未找到候选面板";
        if (!string.IsNullOrEmpty(lastError))
            note += $"；UIA异常={lastError}";
        return new ScanResult(null, hostPids.Count, windows.Count, inspected, panes,
            $"{note} · {diagnostics}");
    }

    private static bool TryRead(IntPtr hwnd, out CandidateHit? hit, out bool paneSeen)
    {
        hit = null;
        paneSeen = false;
        var root = AutomationElement.FromHandle(hwnd);
        if (root is null) return false;

        // IMPORTANT: Win10 19045 report found the candidate pane in the RAW tree.
        // Avoid an expensive FindFirst(TreeScope.Descendants) on unrelated app windows.
        var pane = FindCandidatePaneInRawTree(root);
        if (pane is null) return false;
        paneSeen = true;

        Rect paneRect = pane.Current.BoundingRectangle;
        if (!Valid(paneRect)) return false;

        var first = FindFirstListItemInRawTree(pane);
        if (first is null) return false;

        string candidate = first.Current.Name?.Trim() ?? "";
        if (!HasHan(candidate))
        {
            var text = FindCandidateTextInRawTree(first);
            candidate = text?.Current.Name?.Trim() ?? "";
        }
        if (!HasHan(candidate)) return false;

        Rect itemRect = first.Current.BoundingRectangle;
        double x = Valid(itemRect) ? itemRect.Left : paneRect.Left;
        double width = Math.Max(20, paneRect.Right - x);
        var anchor = new Rect(x, paneRect.Top, width, paneRect.Height);
        hit = new CandidateHit(candidate, anchor,
            $"{pane.Current.AutomationId} · HWND=0x{hwnd.ToInt64():X}");
        return true;
    }

    private static bool IsCandidatePane(AutomationElement e)
    {
        string id = e.Current.AutomationId;
        return id is "IME_Prediction_Window" or "IME_Candidate_Window";
    }

    private static AutomationElement? FindCandidatePaneInRawTree(AutomationElement root)
    {
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Node, int Depth)>();
        queue.Enqueue((root, 0));
        int visited = 0;
        while (queue.Count > 0 && visited++ < 80)
        {
            var (node, depth) = queue.Dequeue();
            if (IsCandidatePane(node)) return node;
            if (depth < 5) EnqueueChildren(walker, node, depth, queue, 20);
        }
        return null;
    }

    private static AutomationElement? FindFirstListItemInRawTree(AutomationElement pane)
    {
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Node, int Depth)>();
        EnqueueChildren(walker, pane, 0, queue, 30);
        int visited = 0;
        while (queue.Count > 0 && visited++ < 65)
        {
            var (node, depth) = queue.Dequeue();
            if (node.Current.ControlType == ControlType.ListItem) return node;
            if (depth < 3) EnqueueChildren(walker, node, depth, queue, 24);
        }
        return null;
    }

    private static AutomationElement? FindCandidateTextInRawTree(AutomationElement first)
    {
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Node, int Depth)>();
        EnqueueChildren(walker, first, 0, queue, 14);
        while (queue.Count > 0)
        {
            var (node, depth) = queue.Dequeue();
            if (node.Current.AutomationId == "TEMPLATE_PART_CandidateItemText") return node;
            if (depth < 2) EnqueueChildren(walker, node, depth, queue, 12);
        }
        return null;
    }

    private static void EnqueueChildren(TreeWalker walker, AutomationElement parent,
        int depth, Queue<(AutomationElement Node, int Depth)> queue, int max)
    {
        AutomationElement? child = walker.GetFirstChild(parent);
        for (int i = 0; child is not null && i < max; i++)
        {
            queue.Enqueue((child, depth + 1));
            child = walker.GetNextSibling(child);
        }
    }

    private static HashSet<uint> FindHostPids()
    {
        var ids = new HashSet<uint>();
        foreach (string name in HostNames)
        {
            try
            {
                foreach (Process p in Process.GetProcessesByName(name))
                {
                    using (p)
                    {
                        try { ids.Add((uint)p.Id); }
                        catch (InvalidOperationException) { }
                    }
                }
            }
            catch (Exception ex) when (ex is Win32Exception or InvalidOperationException) { }
        }
        return ids;
    }

    private sealed record NativeWindow(IntPtr Handle, int Priority, int Height);

    private static List<NativeWindow> FindCandidateWindows(HashSet<uint> hosts,
        out int coreWindowCount, out int hostWindowCount)
    {
        var seen = new HashSet<IntPtr>();
        var matches = new List<NativeWindow>();
        int core = 0, host = 0;

        void Add(IntPtr hwnd)
        {
            if (!seen.Add(hwnd)) return;
            if (!IsWindowVisible(hwnd) || !GetWindowRect(hwnd, out NativeRect rect)) return;
            int height = rect.Bottom - rect.Top;
            if (height < 20 || rect.Right - rect.Left < 30) return;

            string cls = ReadClassName(hwnd);
            bool coreWindow = cls.Equals("Windows.UI.Core.CoreWindow", StringComparison.Ordinal);
            _ = GetWindowThreadProcessId(hwnd, out uint pid);
            bool hostWindow = hosts.Contains(pid);
            if (coreWindow) core++;
            if (hostWindow) host++;
            if (!coreWindow && !hostWindow) return;

            // First inspect exact signature from user's successful Windows 10 log.
            // Win11 can use a different class, so all windows of its TextInputHost
            // remain eligible, but after CoreWindow targets.
            int priority = coreWindow && hostWindow ? 0 : coreWindow ? 1 : 2;
            matches.Add(new NativeWindow(hwnd, priority, height));
        }

        // EnumChildWindows includes descendants: essential because TextInputHost's
        // CoreWindow is a native child of explorer's ApplicationFrameWindow on Win10.
        EnumWindows((top, _) =>
        {
            Add(top);
            EnumChildWindows(top, (child, _) => { Add(child); return true; }, IntPtr.Zero);
            return true;
        }, IntPtr.Zero);

        coreWindowCount = core;
        hostWindowCount = host;
        return matches.OrderBy(w => w.Priority)
            .ThenBy(w => w.Height <= 250 ? 0 : 1)
            .ThenBy(w => w.Height)
            .ToList();
    }

    private static string ReadClassName(IntPtr hwnd)
    {
        var buffer = new StringBuilder(160);
        _ = GetClassName(hwnd, buffer, buffer.Capacity);
        return buffer.ToString();
    }

    private static bool HasHan(string text)
    {
        foreach (char c in text)
            if (c is >= '\u3400' and <= '\u9FFF') return true;
        return false;
    }

    private static bool Valid(Rect r) => !r.IsEmpty &&
        !double.IsInfinity(r.X) && !double.IsNaN(r.X) &&
        r.Width >= 10 && r.Height >= 10;

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeRect { public int Left, Top, Right, Bottom; }
    private delegate bool WindowCallback(IntPtr hwnd, IntPtr lparam);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumWindows(WindowCallback callback, IntPtr lparam);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumChildWindows(IntPtr parent, WindowCallback callback, IntPtr lparam);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr hwnd, StringBuilder className, int maxCount);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetWindowRect(IntPtr hwnd, out NativeRect rect);
}
