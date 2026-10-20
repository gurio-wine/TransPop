using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace PinyinCandidateProbe;

public partial class CandidateOverlay : Window
{
    private const int GwlExstyle = -20;
    private const int WsExTransparent = 0x00000020;
    private const int WsExToolwindow = 0x00000080;
    private const int WsExNoactivate = 0x08000000;

    public CandidateOverlay()
    {
        InitializeComponent();
        IsHitTestVisible = false;
        SourceInitialized += (_, _) =>
        {
            var hwnd = new WindowInteropHelper(this).Handle;
            var current = GetWindowLong(hwnd, GwlExstyle);
            SetWindowLong(hwnd, GwlExstyle,
                current | WsExTransparent | WsExToolwindow | WsExNoactivate);
        };
    }

    // UIA 返回的是屏幕物理像素；WPF Window.Left/Top 则使用 DIP。
    // 使用当前 PresentationSource 的转换矩阵做基本的 DPI 处理。
    public void Update(string candidate, Rect candidateRectPixels)
    {
        CandidateLabel.Text = candidate;
        if (!IsVisible)
            Show();

        var presentation = PresentationSource.FromVisual(this);
        var converter = presentation?.CompositionTarget?.TransformFromDevice ?? System.Windows.Media.Matrix.Identity;
        var position = converter.Transform(new Point(candidateRectPixels.Left, candidateRectPixels.Bottom + 5));
        Left = position.X;
        Top = position.Y;
    }

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW", SetLastError = true)]
    private static extern int GetWindowLong(IntPtr hwnd, int index);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW", SetLastError = true)]
    private static extern int SetWindowLong(IntPtr hwnd, int index, int newStyle);
}
