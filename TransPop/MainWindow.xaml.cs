using System;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Threading;
using TransPop.Services;

namespace TransPop;

public partial class MainWindow : Window
{
    private readonly CandidateScanner _scanner = new();
    private readonly DispatcherTimer _timer;
    private readonly CandidateOverlay _overlay = new();
    private int _scanning;
    private bool _isClosing;
    private bool _manualActive;

    public MainWindow()
    {
        InitializeComponent();
        _timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(800) };
        _timer.Tick += async (_, _) => await ScanAsync(false);
        Loaded += (_, _) => _timer.Start();
        Closed += (_, _) =>
        {
            _isClosing = true;
            _timer.Stop();
            _overlay.Close();
        };
    }

    private async void PointerScanButton_Click(object sender, RoutedEventArgs e)
    {
        await RunManualScanAsync(true);
    }

    private async void DeepScanButton_Click(object sender, RoutedEventArgs e)
    {
        await RunManualScanAsync(false);
    }

    private async Task RunManualScanAsync(bool pointer)
    {
        if (_manualActive || _isClosing) return;
        _manualActive = true;
        _timer.Stop();
        PointerScanButton.IsEnabled = false;
        DeepScanButton.IsEnabled = false;
        try
        {
            if (pointer)
            {
                PointerSummaryText.Text = "5 秒倒计时：切回记事本，打开拼音候选，将鼠标停在第一候选上…";
                await Task.Delay(5000);
                if (_isClosing) return;
                PointerReport report = await Task.Run(PointerDiagnostics.Capture);
                if (_isClosing) return;
                PointerSummaryText.Text = report.Summary;
                ReportBox.Text = report.Details;
                StatusText.Text = "已完成鼠标定位诊断；请检查下方窗口类名和 UIA 节点。";
            }
            else
            {
                StatusText.Text = "4 秒内切回记事本并打开拼音候选…";
                await Task.Delay(4000);
                if (!_isClosing) await ScanAsync(true);
            }
        }
        catch (Exception ex)
        {
            if (!_isClosing)
            {
                PointerSummaryText.Text = $"诊断异常：{ex.GetType().Name}";
                ReportBox.Text = ex.ToString();
            }
        }
        finally
        {
            _manualActive = false;
            if (!_isClosing)
            {
                PointerScanButton.IsEnabled = true;
                DeepScanButton.IsEnabled = true;
                _timer.Start();
            }
        }
    }

    private void CopyReportButton_Click(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(ReportBox.Text) || ReportBox.Text == "暂无诊断信息") return;
        try
        {
            Clipboard.SetText(ReportBox.Text);
            PointerSummaryText.Text = "诊断信息已复制；分享前请检查是否包含隐私文本。";
        }
        catch (Exception ex)
        {
            PointerSummaryText.Text = "复制失败：" + ex.Message;
        }
    }

    private async Task ScanAsync(bool thorough)
    {
        if (Interlocked.Exchange(ref _scanning, 1) != 0) return;
        try
        {
            if (_isClosing) return;
            var result = await Task.Run(() => _scanner.Scan(thorough));
            if (_isClosing) return;

            if (result.Hit is { } hit)
            {
                StatusText.Text = $"扫描命中（{result.Inspected}/{result.WindowCount} 个顶层窗口）";
                CandidateText.Text = "第一候选：" + hit.FirstCandidate;
                LastSeenText.Text = $"最近检测：{DateTime.Now:HH:mm:ss} · {hit.FirstCandidate}";
                BoundsText.Text = $"候选区域：X={hit.Bounds.X:0}, Y={hit.Bounds.Y:0}, " +
                                  $"W={hit.Bounds.Width:0}, H={hit.Bounds.Height:0}";
                _overlay.Update(hit.FirstCandidate, hit.Bounds);
            }
            else
            {
                StatusText.Text = result.Note + $"（{result.Inspected}/{result.WindowCount}）";
                CandidateText.Text = "第一候选：尚未检测到";
                BoundsText.Text = "位置：—";
                _overlay.Hide();
            }
        }
        catch (Exception ex)
        {
            if (!_isClosing)
            {
                StatusText.Text = $"自动扫描异常：{ex.GetType().Name} — {ex.Message}";
                _overlay.Hide();
            }
        }
        finally { Interlocked.Exchange(ref _scanning, 0); }
    }
}
