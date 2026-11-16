param([string]$Exe = (Join-Path (Split-Path $PSScriptRoot -Parent) 'dist\SC-MiningAssist.exe'))
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (Get-Process SC-MiningAssist,MiningMod -ErrorAction SilentlyContinue) {
    throw 'Close SC-MiningAssist and any older preview before the lifecycle test.'
}
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MiningLifecycle {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
    public static IntPtr MainWindow() { return FindWindowW("MiningMod.Main", null); }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, UIntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr hwnd, uint msg, UIntPtr wp, IntPtr lp, uint flags, uint timeout, out UIntPtr result);
    public static bool CloseViaTitleBar(IntPtr hwnd) { UIntPtr result; return SendMessageTimeoutW(hwnd,0x0112,(UIntPtr)0xF060,IntPtr.Zero,2,2000,out result) != IntPtr.Zero; }
    public static bool Minimize(IntPtr hwnd) { UIntPtr result; return SendMessageTimeoutW(hwnd,0x0112,(UIntPtr)0xF020,IntPtr.Zero,2,2000,out result) != IntPtr.Zero; }
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindowExW(IntPtr parent,IntPtr after,string cls,string name);
    public static bool MenuVisible() { return MenuWindow() != IntPtr.Zero; }
    public static IntPtr MenuWindow() {
        IntPtr menu = IntPtr.Zero;
        uint owner; GetWindowThreadProcessId(MainWindow(),out owner);
        while ((menu = FindWindowExW(IntPtr.Zero,menu,"#32768",null)) != IntPtr.Zero) {
            uint pid; GetWindowThreadProcessId(menu,out pid);
            if (pid == owner && IsWindowVisible(menu)) return menu;
        }
        return IntPtr.Zero;
    }
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int L,T,R,B; }
    [StructLayout(LayoutKind.Sequential)] struct MenuInfo { public uint Size; public Rect Rect; public IntPtr Menu,Window; public uint Flags; }
    [DllImport("user32.dll")] static extern bool GetMenuBarInfo(IntPtr hwnd,int obj,int item,ref MenuInfo info);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu,int index);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetMenuStringW(IntPtr menu,uint item,StringBuilder text,int count,uint flags);
    public static uint QuitCommand() {
        var info = new MenuInfo { Size=(uint)Marshal.SizeOf(typeof(MenuInfo)) };
        if (!GetMenuBarInfo(MenuWindow(),-4,0,ref info)) return 0;
        for (int i=0; i<GetMenuItemCount(info.Menu); i++) {
            var text = new StringBuilder(100);
            GetMenuStringW(info.Menu,(uint)i,text,100,0x400);
            if (text.ToString() == "Quit SC-MiningAssist") return GetMenuItemID(info.Menu,i);
        }
        return 0;
    }
}
'@
function Wait-Condition([scriptblock]$Condition, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $Failure
}
function Main-Window { [MiningLifecycle]::MainWindow() }
$children = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
try {
    $first = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
    $children.Add($first)
    Wait-Condition { (Main-Window) -ne [IntPtr]::Zero } 'Initial window not found.'
    for ($attempt=0; $attempt -lt 3; $attempt++) {
        if (-not [MiningLifecycle]::Minimize((Main-Window))) { throw 'Minimize command timed out.' }
        Wait-Condition { [MiningLifecycle]::IsIconic((Main-Window)) } 'Window did not minimize.'
        if ($first.HasExited) { throw 'Main process exited unexpectedly.' }
        $second = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
        $children.Add($second)
        if (-not $second.WaitForExit(5000)) { throw 'Second launch did not exit.' }
        Wait-Condition { [MiningLifecycle]::IsWindowVisible((Main-Window)) -and -not [MiningLifecycle]::IsIconic((Main-Window)) } 'Second launch did not reopen settings.'
        [uint32]$ownerPid = 0
        [void][MiningLifecycle]::GetWindowThreadProcessId((Main-Window),[ref]$ownerPid)
        if ($ownerPid -ne $first.Id) { throw 'Two concurrent instances were created.' }
    }
    [void][MiningLifecycle]::PostMessageW((Main-Window),0x0111,[UIntPtr]105,[IntPtr]::Zero)
    if (-not $first.WaitForExit(5000)) { throw 'Quit did not terminate the process.' }
    Wait-Condition { (Main-Window) -eq [IntPtr]::Zero } 'A window remains after Quit.'

    # Two immediate launches also cover the interval before the first window exists.
    $a = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
    $children.Add($a)
    $b = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
    $children.Add($b)
    Wait-Condition { (Main-Window) -ne [IntPtr]::Zero } 'Restart after Quit failed.'
    Wait-Condition { $a.HasExited -xor $b.HasExited } 'Double launch did not leave exactly one instance.'
    $owner = if ($a.HasExited) { $b } else { $a }
    # Taskbar Close / title-bar X route must now really quit.
    if (-not [MiningLifecycle]::CloseViaTitleBar((Main-Window))) { throw 'Close command timed out.' }
    if (-not $owner.WaitForExit(5000)) { throw 'Taskbar Close did not exit.' }

    $tray = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
    $children.Add($tray)
    Wait-Condition { (Main-Window) -ne [IntPtr]::Zero } 'Window not found for menu test.'
    # Resolve Quit from the actual native menu, then dispatch its command while
    # TrackPopupMenu is active. This exercises nested-loop shutdown, not key injection.
    [void][MiningLifecycle]::PostMessageW((Main-Window),0x8001,[UIntPtr]1,[IntPtr]0x0205)
    Wait-Condition { [MiningLifecycle]::MenuVisible() } 'Tray menu did not open.'
    Wait-Condition { [MiningLifecycle]::QuitCommand() -eq 105 } 'Quit command missing from tray menu.'
    [void][MiningLifecycle]::PostMessageW((Main-Window),0x0111,[UIntPtr]([MiningLifecycle]::QuitCommand()),[IntPtr]::Zero)
    if (-not $tray.WaitForExit(5000)) { throw 'Tray menu Quit did not terminate the process.' }
    Write-Host 'OK: minimize/reopen, Quit, window/taskbar Close, tray menu and double launch.'
} finally {
    # Clean up only processes created by this test, never an unrelated instance.
    foreach ($child in $children) { if (-not $child.HasExited) { Stop-Process -Id $child.Id -ErrorAction SilentlyContinue } }
}
