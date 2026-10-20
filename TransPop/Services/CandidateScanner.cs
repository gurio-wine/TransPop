using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Automation;

namespace TransPop.Services;

public sealed record CandidateHit(string FirstCandidate, Rect Anchor, string Source);
public sealed record ScanResult(CandidateHit? Hit, int Hosts, int Windows, int Inspected, int Panes, string Note);

/// <summary>
/// Mouse-free, read-only Microsoft Pinyin probe. Find HWNDs owned by the
/// Windows text input processes, then examine ONLY their UIA subtrees.
/// This deliberately never reads the user's editor, clipboard or keystrokes.
/// Run Scan on a thread-pool (MTA) worker, not the WPF dispatcher thread.
/// </summary>
public sealed class CandidateScanner
{
    private static readonly string[] HostProcesses = ["TextInputHost", "InputApp", "ctfmon"];
    public ScanResult Scan()
    {
        HashSet<uint> pids = FindHostProcesses();
        if (pids.Count == 0)
            return new(null, 0, 0, 0, 0, "没有找到微软输入法宿主进程（TextInputHost/InputApp）");

        List<IntPtr> hwnds = FindWindowsOwnedBy(pids);
        int inspected = 0;
        int panes = 0;
        foreach (IntPtr hwnd in hwnds)
        {
            if (!IsWindowVisible(hwnd) || !GetWindowRect(hwnd, out NativeRect r) ||
                r.Right <= r.Left || r.Bottom <= r.Top)
                continue;

            inspected++;
            try
            {
                if (TryRead(hwnd, out CandidateHit? hit, out bool paneSeen))
                {
                    panes++;
                    return new(hit, pids.Count, hwnds.Count, inspected, panes, "已识别微软拼音第一候选");
                }
                if (paneSeen) panes++;
            }
            catch (Exception ex) when (ex is COMException or ElementNotAvailableException or
                                       InvalidOperationException or UnauthorizedAccessException)
            {
                // UIA elements are transient while composing: try again on next tick.
            }
        }
        string note = panes > 0
            ? "找到了候选容器，但没有可读的中文候选"
            : inspected > 0
                ? "已找到输入法宿主窗口，等待候选框出现"
                : "找到输入法进程，但没有可见的候选窗口";
        return new(null, pids.Count, hwnds.Count, inspected, panes, note);
    }

    private static bool TryRead(IntPtr hwnd, out CandidateHit? hit, out bool paneSeen)
    {
        hit = null;
        paneSeen = false;
        AutomationElement? root = AutomationElement.FromHandle(hwnd);
        if (root is null) return false;

        // Usually fast: Window 10 19045: root CoreWindow -> IME_Prediction_Window.
        AutomationElement? pane = IsCandidatePane(root)
            ? root
            : root.FindFirst(TreeScope.Descendants, new OrCondition(
                new PropertyCondition(AutomationElement.AutomationIdProperty, "IME_Prediction_Window"),
                new PropertyCondition(AutomationElement.AutomationIdProperty, "IME_Candidate_Window")));

        // Important fallback: some UIA providers expose the pane in RAW view only.
        pane ??= FindPaneInRawTree(root);
        if (pane is null) return false;
        paneSeen = true;
        if (pane.Current.IsOffscreen) return false;
        Rect paneRect = pane.Current.BoundingRectangle;
        if (!Valid(paneRect)) return false;

        var itemCondition = new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.ListItem);
        AutomationElement? first = pane.FindFirst(TreeScope.Children, itemCondition)
            ?? pane.FindFirst(TreeScope.Descendants, itemCondition);
        first ??= FindFirstItemInRawTree(pane);
        if (first is null || first.Current.IsOffscreen) return false;

        // The first ListItem.Name is '你好' in the real Windows 10 report.
        string text = first.Current.Name?.Trim() ?? "";
        if (!HasHan(text))
        {
            // Some providers leave the ListItem.Name blank but expose child TextBlock.
            var child = first.FindFirst(TreeScope.Descendants,
                new PropertyCondition(AutomationElement.AutomationIdProperty,
                    "TEMPLATE_PART_CandidateItemText"));
            text = child?.Current.Name?.Trim() ?? "";
        }
        if (!HasHan(text)) return false;

        Rect firstRect = first.Current.BoundingRectangle;
        double x = Valid(firstRect) ? firstRect.Left : paneRect.Left;
        // Bubble below entire IME candidate popup, aligned with candidate #1.
        var anchor = new Rect(x, paneRect.Top, Math.Max(20, paneRect.Right - x), paneRect.Height);
        hit = new CandidateHit(text, anchor,
            $"{pane.Current.AutomationId} · HWND 0x{hwnd.ToInt64():X}");
        return true;
    }

    private static bool IsCandidatePane(AutomationElement element)
    {
        string? id = element.Current.AutomationId;
        return id is "IME_Prediction_Window" or "IME_Candidate_Window";
    }

    private static AutomationElement? FindPaneInRawTree(AutomationElement root)
    {
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Element, int Depth)>();
        queue.Enqueue((root, 0));
        int visited = 0;
        while (queue.Count > 0 && visited++ < 90)
        {
            var (element, depth) = queue.Dequeue();
            if (IsCandidatePane(element)) return element;
            if (depth >= 4) continue;
            EnqueueChildren(walker, element, depth, queue);
        }
        return null;
    }

    private static AutomationElement? FindFirstItemInRawTree(AutomationElement pane)
    {
        var walker = TreeWalker.RawViewWalker;
        var queue = new Queue<(AutomationElement Element, int Depth)>();
        queue.Enqueue((pane, 0));
        int visited = 0;
        while (queue.Count > 0 && visited++ < 90)
        {
            var (element, depth) = queue.Dequeue();
            if (depth > 0 && element.Current.ControlType == ControlType.ListItem) return element;
            if (depth >= 3) continue;
            EnqueueChildren(walker, element, depth, queue);
        }
        return null;
    }

    private static void EnqueueChildren(TreeWalker walker, AutomationElement parent, int parentDepth,
        Queue<(AutomationElement Element, int Depth)> queue)
    {
        AutomationElement? child = walker.GetFirstChild(parent);
        for (int i = 0; child is not null && i < 24; i++)
        {
            queue.Enqueue((child, parentDepth + 1));
            child = walker.GetNextSibling(child);
        }
    }

    private static HashSet<uint> FindHostProcesses()
    {
        var ids = new HashSet<uint>();
        foreach (string name in HostProcesses)
        {
            Process[] processes;
            try { processes = Process.GetProcessesByName(name); }
            catch (Exception ex) when (ex is Win32Exception or InvalidOperationException) { continue; }
            foreach (var process in processes)
            {
                using (process)
                {
                    try { ids.Add(checked((uint)process.Id)); }
                    catch (InvalidOperationException) { }
                }
            }
        }
        return ids;
    }

    private static List<IntPtr> FindWindowsOwnedBy(HashSet<uint> pids)
    {
        var found = new List<IntPtr>();
        var seen = new HashSet<IntPtr>();
        void AddIfMatch(IntPtr hwnd)
        {
            _ = GetWindowThreadProcessId(hwnd, out uint pid);
            if (pids.Contains(pid) && seen.Add(hwnd)) found.Add(hwnd);
        }
        // Win10's CoreWindow can be under explorer's ApplicationFrameWindow.
        // Enumerate all NATIVE child HWNDs, but inspect UIA only for host PIDs.
        EnumWindows((root, _) =>
        {
            AddIfMatch(root);
            EnumChildWindows(root, (child, _) => { AddIfMatch(child); return true; }, IntPtr.Zero);
            return true;
        }, IntPtr.Zero);
        return found;
    }

    private static bool HasHan(string text)
    {
        foreach (char c in text)
            if (c is >= '\u3400' and <= '\u9FFF') return true;
        return false;
    }

    private static bool Valid(Rect r) => !r.IsEmpty && r.Width >= 10 && r.Height >= 10;

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

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetWindowRect(IntPtr hwnd, out NativeRect rect);
}
