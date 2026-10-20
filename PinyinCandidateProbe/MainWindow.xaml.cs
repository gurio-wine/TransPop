using System;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Threading;
using PinyinCandidateProbe.Services;

namespace PinyinCandidateProbe;

public partial class MainWindow : Window
{
    private readonly CandidateScanner _scanner = new();
    private readonly DispatcherTimer _timer;
    private readonly CandidateOverlay _overlay = new();
    private int _scanning;
    private bool _isClosing;

    public MainWindow()
    {
        InitializeComponent();
        _timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(650) };
        _timer.Tick += async (_, _) => await ScanAsync(false);
        Loaded += (_, _) => _timer.Start();
        Closed += (_, _) =>
        {
            _isClosing = true;
            _timer.Stop();
            _overlay.Close();
        };
    }

    private async void DeepScanButton_Click(object sender, RoutedEventArgs e)
    {
        // 点击按钮本身会使微软拼音的候选窗消失：延时让用户切回记事本。
        _timer.Stop();
        DeepScanButton.IsEnabled = false;
        StatusText.Text = "请在 4 秒内切回记事本、输入拼音并保持候选栏打开…";
        try
        {
            await Task.Delay(4000);
            if (!_isClosing)
                await ScanAsync(true);
        }
        finally
        {
            if (!_isClosing)
            {
                DeepScanButton.IsEnabled = true;
                _timer.Start();
            }
        }
    }

    private async Task ScanAsync(bool thorough)
    {
        // UIA 可能较慢；不要同时运行多个扫描。
        if (Interlocked.Exchange(ref _scanning, 1) != 0) return;
        if (_isClosing) return;

        try
        {
            if (thorough)
            {
                DeepScanButton.IsEnabled = false;
                StatusText.Text = "正在扫描可见窗口的 UIA 树…";
            }

            var result = await Task.Run(() => _scanner.Scan(thorough));
            if (_isClosing) return;

            if (result.Hit is { } hit)
            {
                StatusText.Text = $"已检测到（扫描 {result.Inspected}/{result.WindowCount} 个窗口）";
                CandidateText.Text = "第一候选：" + hit.FirstCandidate;
                LastSeenText.Text = $"最近检测记录：{DateTime.Now:HH:mm:ss} · {hit.FirstCandidate}";
                BoundsText.Text = $"坐标（屏幕像素）：X={hit.Bounds.X:0}  Y={hit.Bounds.Y:0} " +
                                  $"W={hit.Bounds.Width:0}  H={hit.Bounds.Height:0}";
                _overlay.Update(hit.FirstCandidate, hit.Bounds);
            }
            else
            {
                StatusText.Text = result.Note + $"（扫描 {result.Inspected}/{result.WindowCount}）";
                CandidateText.Text = "第一候选：尚未检测到";
                BoundsText.Text = "位置：—";
                _overlay.Hide();
            }
        }
        catch (Exception ex)
        {
            if (_isClosing) return;
            StatusText.Text = "扫描出错：" + ex.GetType().Name + " — " + ex.Message;
            _overlay.Hide();
        }
        finally
        {
            if (!_isClosing) DeepScanButton.IsEnabled = true;
            Interlocked.Exchange(ref _scanning, 0);
        }
    }
}
