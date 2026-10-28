# Captures the Next Launcher window for design review.
#
#   powershell -ExecutionPolicy Bypass -File tools/screenshot.ps1 -Out shot.png [-Route settings:general] [-Width 1280 -Height 800]
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Route = "",
  [string]$Exe = "target\release\next-launcher.exe",
  [string]$DataDir = ".dev-data\release",
  [int]$Wait = 3
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
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
}
"@
[NlWin]::SetProcessDPIAware() | Out-Null

$argsList = @("--data-dir", $DataDir)
if ($Route) { $argsList += @("--dev-route", $Route) }
$p = Start-Process -FilePath $Exe -ArgumentList $argsList -PassThru
Start-Sleep -Seconds $Wait
$p.Refresh()
$h = $p.MainWindowHandle
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
Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
Write-Host "saved $Out (${w}x${hh})"
