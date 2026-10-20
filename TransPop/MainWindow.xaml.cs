using System;
using System.Text;
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
    private readonly CandidateOverlay _bubble = new();
    private readonly StringBuilder _events = new();
    private int _busy;
    private DateTime _scanStartedUtc = DateTime.MinValue;
    private bool _slowScanReported;
    private bool _closing;
    private bool _paused;
    private string _lastState = "";
    private string _lastHit = "";

    public MainWindow()
    {
        InitializeComponent();
        _timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(350) };
        _timer.Tick += async (_, _) => await ScanAsync();
        Loaded += (_, _) =>
        {
            Log("程序启动；后台每 350ms 自动定位 CoreWindow 并检查候选。");
            _timer.Start();
        };
        Closed += (_, _) =>
        {
            _closing = true;
            _timer.Stop();
            _bubble.Close();
        };
    }

    private void PauseButton_Click(object sender, RoutedEventArgs e)
    {
        _paused = !_paused;
        if (_paused)
        {
            _timer.Stop();
            _bubble.Hide();
            PauseButton.Content = "恢复检测";
            StatusText.Text = "检测已暂停";
            Log("手动暂停检测。");
        }
        else
        {
            PauseButton.Content = "暂停检测";
            StatusText.Text = "检测已恢复，等待微软拼音候选框…";
            Log("已恢复自动检测。");
            _timer.Start();
        }
    }

    private void CopyReportButton_Click(object sender, RoutedEventArgs e)
    {
        try { Clipboard.SetText(ReportBox.Text); }
        catch (Exception ex) { StatusText.Text = "复制失败：" + ex.Message; }
    }

    private async Task ScanAsync()
    {
        if (_closing || _paused) return;
        if (Interlocked.CompareExchange(ref _busy, 1, 0) != 0)
        {
            if (!_slowScanReported && (DateTime.UtcNow - _scanStartedUtc).TotalSeconds > 3)
            {
                _slowScanReported = true;
                StatusText.Text = "UI Automation 查询超过 3 秒；请复制下方诊断信息";
                Log("本次 UIA 查询超过 3 秒，还没有返回。可能遇到不响应的候选窗口。 ");
            }
            return;
        }
        _scanStartedUtc = DateTime.UtcNow;
        _slowScanReported = false;
        StatusText.Text = "正在检查原生候选窗口…";
        try
        {
            // UI Automation must run off the WPF UI thread; cursor location is
            // NEVER used in normal operation.
            ScanResult result = await Task.Run(() => _scanner.Scan());
            if (_closing || _paused) return;

            string stats = $"宿主进程 {result.Hosts} / 候选目标 {result.Windows} / 已检查 {result.Inspected} / 命中容器 {result.Panes}";
            if (result.Hit is { } hit)
            {
                StatusText.Text = "检测成功 · " + stats;
                CandidateText.Text = "第一候选：" + hit.FirstCandidate;
                LastSeenText.Text = "最近检测到：" + DateTime.Now.ToString("HH:mm:ss") + " · " + hit.FirstCandidate;
                BoundsText.Text = $"候选窗口：X={hit.Anchor.X:0}, Y={hit.Anchor.Y:0}, H={hit.Anchor.Height:0} · {hit.Source}";
                _bubble.Update(hit.FirstCandidate, hit.Anchor);

                string state = $"{hit.Source} / {hit.FirstCandidate}";
                if (state != _lastHit)
                {
                    Log($"命中第一候选：{hit.FirstCandidate} ({stats}) {hit.Source}");
                    _lastHit = state;
                }
                _lastState = "hit";
            }
            else
            {
                StatusText.Text = result.Note + " · " + stats;
                CandidateText.Text = "第一候选：等待输入";
                BoundsText.Text = "坐标：—";
                _bubble.Hide();
                string state = result.Note + " / " + stats;
                if (state != _lastState)
                {
                    Log(state);
                    _lastState = state;
                }
                _lastHit = "";
            }
        }
        catch (Exception ex)
        {
            if (!_closing)
            {
                StatusText.Text = "扫描异常：" + ex.GetType().Name;
                _bubble.Hide();
                Log("扫描异常：" + ex);
            }
        }
        finally { Interlocked.Exchange(ref _busy, 0); }
    }

    private void Log(string message)
    {
        _events.Append('[').Append(DateTime.Now.ToString("HH:mm:ss"))
            .Append("] ").AppendLine(message);
        // Cap unbounded log growth while running all day.
        if (_events.Length > 16000)
        {
            string tail = _events.ToString();
            _events.Clear();
            _events.Append(tail[^12000..]);
        }
        ReportBox.Text = $"TransPop v0.5 | Windows build {Environment.OSVersion.Version.Build}\n" +
                         _events;
        ReportBox.ScrollToEnd();
    }
}
