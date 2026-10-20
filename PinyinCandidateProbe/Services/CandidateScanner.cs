using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text.RegularExpressions;
using System.Windows;
using System.Windows.Automation;

namespace PinyinCandidateProbe.Services;

public sealed record CandidateHit(string FirstCandidate, Rect Bounds, string Source);
public sealed record ScanResult(CandidateHit? Hit, int WindowCount, int Inspected, string Note);

/// <summary>
/// 只找系统通过 UI Automation 公布的 IME 候选列表，不读取普通编辑框、不模拟输入。
/// 在 ThreadPool 的 MTA 线程执行，避免 UI Automation 阻塞 WPF 界面。
/// </summary>
public sealed class CandidateScanner
{
    private const string CandidateAutomationId = "IME_Candidate_Window";
    private static readonly PropertyCondition CandidateCondition = new(
        AutomationElement.AutomationIdProperty, CandidateAutomationId);

    // 仅扫描疑似输入法窗口；当 UIA 自动识别不到时，用户可手动触发全量扫描。
    private static readonly string[] ImeHints =
    [
        "ime", "candidate", "textinput", "inputmethod", "msctf",
        "拼音", "候选", "chinese", "chsime", "inputapp", "popuphost"
    ];

    public ScanResult Scan(bool thorough)
    {
        var windowsFound = 0;
        var windowsInspected = 0;
        try
        {
            var windows = AutomationElement.RootElement.FindAll(TreeScope.Children, System.Windows.Automation.Condition.TrueCondition);
            windowsFound = windows.Count;

            foreach (AutomationElement window in windows)
            {
                if (!IsVisible(window) || IsOwnWindow(window))
                    continue;

                if (!thorough && !LooksLikeIme(window))
                    continue;

                windowsInspected++;
                if (TryGetHit(window, out var hit))
                    return new ScanResult(hit, windowsFound, windowsInspected, "检测到候选列表");
            }

            return new ScanResult(null, windowsFound, windowsInspected,
                thorough
                    ? "深度扫描结束：没有找到可读取的 IME_Candidate_Window"
                    : "等待候选窗口；必要时点击「手动深度扫描」");
        }
        catch (Exception ex) when (ex is ElementNotAvailableException or InvalidOperationException
                                       or System.Runtime.InteropServices.COMException)
        {
            return new ScanResult(null, windowsFound, windowsInspected,
                $"UI Automation 访问失败：{ex.GetType().Name}");
        }
    }

    private static bool TryGetHit(AutomationElement root, out CandidateHit? hit)
    {
        hit = null;
        try
        {
            // Root 自己有可能就是候选列表。
            AutomationElement? list = root.Current.AutomationId == CandidateAutomationId
                ? root
                : root.FindFirst(TreeScope.Descendants, CandidateCondition);

            if (list is null || !IsVisible(list))
                return false;

            var rect = list.Current.BoundingRectangle;
            if (rect.IsEmpty || rect.Width < 20 || rect.Height < 10)
                rect = root.Current.BoundingRectangle;
            if (rect.IsEmpty || rect.Width < 20 || rect.Height < 10)
                return false;

            string? candidate = ReadFirstChineseCandidate(list);
            if (string.IsNullOrWhiteSpace(candidate))
                return false;

            hit = new CandidateHit(candidate, rect, root.Current.ClassName);
            return true;
        }
        catch (Exception ex) when (ex is ElementNotAvailableException or InvalidOperationException
                                       or System.Runtime.InteropServices.COMException)
        {
            return false;
        }
    }

    private static string? ReadFirstChineseCandidate(AutomationElement list)
    {
        // 优先读取候选项控件；只有未公开候选项时，才回退到普通 Text 控件。
        foreach (var type in new[] { ControlType.ListItem, ControlType.DataItem, ControlType.Text })
        {
            var items = list.FindAll(TreeScope.Descendants,
                new PropertyCondition(AutomationElement.ControlTypeProperty, type));
            foreach (AutomationElement item in items)
            {
                try
                {
                    if (item.Current.IsOffscreen) continue;
                    var text = NormalizeCandidate(item.Current.Name);
                    if (ContainsChinese(text) && !IsGenericLabel(text)) return text;
                }
                catch (ElementNotAvailableException) { }
            }
        }

        // 某些提供程序把候选直接挂在 Pane 节点上。
        var immediate = list.FindAll(TreeScope.Children, System.Windows.Automation.Condition.TrueCondition);
        foreach (AutomationElement item in immediate)
        {
            try
            {
                if (item.Current.IsOffscreen) continue;
                var text = NormalizeCandidate(item.Current.Name);
                if (ContainsChinese(text) && !IsGenericLabel(text)) return text;
            }
            catch (ElementNotAvailableException) { }
        }
        return null;
    }

    private static bool IsGenericLabel(string text) =>
        text is "候选" or "候选词" or "候选窗口" or "拼音" or "微软拼音";

    private static string NormalizeCandidate(string? name)
    {
        if (string.IsNullOrWhiteSpace(name)) return "";
        // 例如 UIA 名称 "1. 你好" -> "你好"。不更改实际输入法候选。
        return Regex.Replace(name.Trim(), @"^\s*[1-9]\s*[.．、:：)]\s*", "").Trim();
    }

    private static bool ContainsChinese(string text) =>
        Regex.IsMatch(text, @"[\u3400-\u9FFF]");

    private static bool IsOwnWindow(AutomationElement element)
    {
        try { return element.Current.ProcessId == Environment.ProcessId; }
        catch { return false; }
    }

    private static bool IsVisible(AutomationElement element)
    {
        try
        {
            return !element.Current.IsOffscreen &&
                !element.Current.BoundingRectangle.IsEmpty;
        }
        catch (ElementNotAvailableException) { return false; }
        catch (System.Runtime.InteropServices.COMException) { return false; }
    }

    private static bool LooksLikeIme(AutomationElement element)
    {
        try
        {
            var info = element.Current;
            if (HasHint(info.AutomationId) || HasHint(info.ClassName) || HasHint(info.Name))
                return true;

            if (info.ProcessId > 0)
            {
                using var process = Process.GetProcessById(info.ProcessId);
                return HasHint(process.ProcessName);
            }
        }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException
                                       or System.ComponentModel.Win32Exception
                                       or ElementNotAvailableException
                                       or System.Runtime.InteropServices.COMException)
        {
        }
        return false;
    }

    private static bool HasHint(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return false;
        foreach (var hint in ImeHints)
        {
            if (value.Contains(hint, StringComparison.OrdinalIgnoreCase))
                return true;
        }
        return false;
    }
}
