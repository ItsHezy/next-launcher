# Captures the Next Launcher window for design review.
#
#   powershell -ExecutionPolicy Bypass -File tools/screenshot.ps1 -Out shot.png [-Route settings:general] [-Width 1280 -Height 800]
#   -Attach captures the running app; -Attach -Title <part> captures another of its windows by title.
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Route = "",
  [string]$Exe = "target\release\next-launcher.exe",
  [string]$DataDir = ".dev-data\release",
  [int]$Wait = 3,
  # Capture the already-running app instead of launching (and closing) a new one.
  [switch]$Attach,
  [string]$Title = ""
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NlWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  /// First visible top-level window of process `pid` whose title contains `part`.
  public static IntPtr Find(uint pid, string part) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid || !IsWindowVisible(h)) return true;
      var sb = new System.Text.StringBuilder(512); GetWindowText(h, sb, 512);
      if (sb.ToString().IndexOf(part, StringComparison.OrdinalIgnoreCase) >= 0) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
}
"@
[NlWin]::SetProcessDPIAware() | Out-Null

if ($Attach) {
  $p = Get-Process next-launcher -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
  if (-not $p) { throw "Next Launcher isn't running" }
} else {
  $argsList = @("--data-dir", $DataDir)
  if ($Route) { $argsList += @("--dev-route", $Route) }
  $p = Start-Process -FilePath $Exe -ArgumentList $argsList -PassThru
  Start-Sleep -Seconds $Wait
  $p.Refresh()
}
$h = if ($Title) { [NlWin]::Find([uint32]$p.Id, $Title) } else { $p.MainWindowHandle }
if ($h -eq [IntPtr]::Zero) { throw "no main window" }
[NlWin]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400

$r = New-Object NlWin+RECT
# Extended frame bounds = the visible window, without the invisible resize border.
if ([NlWin]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) -ne 0) { [NlWin]::GetWindowRect($h, [ref]$r) | Out-Null }
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save((Resolve-Path -LiteralPath (Split-Path -Parent (Join-Path (Get-Location) $Out))).Path + "\" + (Split-Path -Leaf $Out), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
if (-not $Attach) { Stop-Process -Id $p.Id -ErrorAction SilentlyContinue }
Write-Host "saved $Out (${w}x${hh})"
