param([string]$Exe = (Join-Path (Split-Path $PSScriptRoot -Parent) 'dist\SC-MiningAssist.exe'))
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (Get-Process SC-MiningAssist,MiningMod -ErrorAction SilentlyContinue) { throw 'Close SC-MiningAssist and any older preview before running this test.' }
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MiningHud {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo { public int Size; public Rect Bounds, Work; public uint Flags; }
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindowW(string cls,string title);
    public static IntPtr Window(string cls) { return FindWindowW(cls,null); }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd,uint flags);
    [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr monitor,ref MonitorInfo info);
    [DllImport("user32.dll")] public static extern IntPtr GetClassLongPtrW(IntPtr hwnd,int index);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr hwnd,int index);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd,IntPtr dc,uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd,uint msg,UIntPtr wp,IntPtr lp);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hwnd,int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);
}
'@
[void][MiningHud]::SetProcessDpiAwarenessContext([IntPtr](-4))
function Wait-Hud([scriptblock]$Condition) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 30 } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Test window not found.'
}
$process = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
try {
    Wait-Hud { [MiningHud]::Window('MiningMod.Main') -ne [IntPtr]::Zero }
    $main = [MiningHud]::Window('MiningMod.Main')
    if ([MiningHud]::GetClassLongPtrW($main,-14) -eq [IntPtr]::Zero) { throw 'Icon not loaded.' }
    if ([MiningHud]::GetDlgItem($main,110) -ne [IntPtr]::Zero -or [MiningHud]::GetDlgItem($main,111) -ne [IntPtr]::Zero) { throw 'Options remain in the compact UI.' }
    [void][MiningHud]::PostMessageW($main,0x0111,[UIntPtr]103,[IntPtr]::Zero)
    Wait-Hud { [MiningHud]::IsWindowVisible([MiningHud]::Window('MiningMod.Overlay')) }
    $hud = [MiningHud]::Window('MiningMod.Overlay')
    $rect = [MiningHud+Rect]::new()
    [void][MiningHud]::GetWindowRect($hud,[ref]$rect)
    $monitor = [MiningHud+MonitorInfo]::new()
    $monitor.Size = [System.Runtime.InteropServices.Marshal]::SizeOf($monitor)
    [void][MiningHud]::GetMonitorInfoW([MiningHud]::MonitorFromWindow($hud,2),[ref]$monitor)
    $width = $rect.Right-$rect.Left
    $height = $rect.Bottom-$rect.Top
    $expectedHeight = [Math]::Round(28 * [MiningHud]::GetDpiForWindow($hud) / 96)
    if ($height -ne $expectedHeight -or $rect.Top -ne $monitor.Bounds.Top -or [Math]::Abs(($rect.Left+$rect.Right)-($monitor.Bounds.Left+$monitor.Bounds.Right)) -gt 1) {
        throw 'HUD is not centered, has a top margin, or has incorrect height.'
    }
    $styles = [MiningHud]::GetWindowLongPtrW($hud,-20).ToInt64()
    if (($styles -band 0x08080028) -ne 0x08080028) { throw 'HUD is missing click-through, no-focus or topmost styles.' }
    $bitmap = [System.Drawing.Bitmap]::new($width,$height,[System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $hdc = $graphics.GetHdc()
        try {
            if (-not [MiningHud]::PrintWindow($hud,$hdc,3)) { throw 'Offscreen rendering failed.' }
        } finally { $graphics.ReleaseHdc($hdc) }
        $output = Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts'
        New-Item -ItemType Directory -Path $output -Force | Out-Null
        if ($bitmap.GetPixel(10,10).R -eq 0 -and $bitmap.GetPixel(10,10).G -eq 0) { throw 'HUD is blank.' }
        # PrintWindow renders our own HUD through WM_PRINTCLIENT; no desktop capture session.
        $imageName = 'hud.png'
        $bitmap.Save((Join-Path $output $imageName),[System.Drawing.Imaging.ImageFormat]::Png)
        $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($Exe)
        try {
            if ($null -eq $icon) { throw 'Executable icon is missing.' }
            $iconBitmap = $icon.ToBitmap()
            try { $iconBitmap.Save((Join-Path $output 'exe-icon.png'),[System.Drawing.Imaging.ImageFormat]::Png) } finally { $iconBitmap.Dispose() }
        } finally { if ($icon) { $icon.Dispose() } }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    [void][MiningHud]::PostMessageW($main,0x0111,[UIntPtr]105,[IntPtr]::Zero)
    if (-not $process.WaitForExit(5000)) { throw 'Shutdown failed.' }
    Write-Host "OK: HUD ${width}x${height}, centered at the top edge, click-through, embedded icon."
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id }
}
