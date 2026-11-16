param([string]$Exe = (Join-Path (Split-Path $PSScriptRoot -Parent) 'dist\MiningMod.exe'))
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (Get-Process MiningMod -ErrorAction SilentlyContinue) { throw 'Close MiningMod before running this test.' }
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MiningPanel {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindowW(string cls,string name);
    public static IntPtr Main() { return FindWindowW("MiningMod.Main",null); }
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hwnd,int id);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd,StringBuilder s,int n);
    public static string Text(IntPtr hwnd) { var s=new StringBuilder(1024); GetWindowTextW(hwnd,s,1024); return s.ToString(); }
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect r);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int cmd);
    [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd,IntPtr dc,uint flags);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp,uint flags,uint timeout,out IntPtr result);
    public static long Send(IntPtr hwnd,uint msg,long wp,long lp) { IntPtr result; if(SendMessageTimeoutW(hwnd,msg,(IntPtr)wp,(IntPtr)lp,2,2000,out result)==IntPtr.Zero) throw new Exception("Window did not respond"); return result.ToInt64(); }
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);
}
'@
[void][MiningPanel]::SetProcessDpiAwarenessContext([IntPtr](-4))
function Wait-Panel([scriptblock]$Condition,[string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds(6)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 30 } while ([DateTime]::UtcNow -lt $deadline)
    throw $Failure
}
$process = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
try {
    Wait-Panel { [MiningPanel]::Main() -ne [IntPtr]::Zero } 'Window missing.'
    $main = [MiningPanel]::Main()
    $start = [MiningPanel]::GetDlgItem($main,102)
    $status = [MiningPanel]::GetDlgItem($main,107)
    Wait-Panel { [MiningPanel]::Text($status).StartsWith('Ready') } 'Windows OCR did not become ready.'
    if (-not [MiningPanel]::IsWindowEnabled($start)) { throw 'Start remained disabled after OCR initialization.' }
    [void][MiningPanel]::ShowWindow($main,4)
    [void][MiningPanel]::UpdateWindow($main)
    $rect = [MiningPanel+Rect]::new()
    [void][MiningPanel]::GetWindowRect($main,[ref]$rect)
    $scale = [MiningPanel]::GetDpiForWindow($main) / 96
    if (($rect.Right-$rect.Left) -ne [Math]::Round(320*$scale) -or ($rect.Bottom-$rect.Top) -ne [Math]::Round(176*$scale)) { throw 'Unexpected panel size.' }
    $x = $rect.Left + [int](80*$scale); $y = $rect.Top + [int](15*$scale)
    if ([MiningPanel]::Send($main,0x84,0,(($y -band 65535) -shl 16) -bor ($x -band 65535)) -ne 2) { throw 'Header is not draggable.' }
    $bitmap = [System.Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $dc = $graphics.GetHdc()
        try { if (-not [MiningPanel]::PrintWindow($main,$dc,0)) { throw 'Panel rendering failed.' } } finally { $graphics.ReleaseHdc($dc) }
        $output = Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts'
        New-Item -ItemType Directory -Path $output -Force | Out-Null
        $bitmap.Save((Join-Path $output 'panel.png'),[System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    [void][MiningPanel]::Send([MiningPanel]::GetDlgItem($main,106),0xF5,0,0)
    Wait-Panel { [MiningPanel]::IsIconic($main) } 'Custom minimize button failed.'
    [void][MiningPanel]::ShowWindow($main,4)
    [void][MiningPanel]::Send($start,0xF5,0,0)
    Wait-Panel { [MiningPanel]::Text($start) -eq 'Pause scanning' } 'Start button failed.'
    Wait-Panel { -not [MiningPanel]::IsWindowVisible($main) -or [MiningPanel]::IsIconic($main) } 'Starting did not minimize.'
    [void][MiningPanel]::Send($main,0x312,1,0)
    Wait-Panel { [MiningPanel]::Text($start) -eq 'Start scanning' } 'Pause hotkey failed.'
    [void][MiningPanel]::PostMessageW($main,0x8003,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-Panel { [MiningPanel]::IsWindowVisible($main) -and -not [MiningPanel]::IsIconic($main) } 'Panel did not reopen.'
    [void][MiningPanel]::Send([MiningPanel]::GetDlgItem($main,104),0xF5,0,0)
    if (-not $process.WaitForExit(5000)) { throw 'Custom Close button failed.' }
    Write-Host 'OK: panel rendering, drag area, native buttons, start, minimize, pause, reopen and close.'
} finally { if (-not $process.HasExited) { Stop-Process -Id $process.Id } }
