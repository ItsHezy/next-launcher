# Idle footprint: starts Next Launcher, waits, then sums memory and CPU of the app
# process and every WebView2 process it spawned (the same set Task Manager groups).
#
#   powershell -ExecutionPolicy Bypass -File tools/measure-idle.ps1 [-Exe path] [-Settle 15] [-Sample 10] [-DataDir path] [-Route settings:general]
param(
  [string]$Exe = "target\release\next-launcher.exe",
  [int]$Settle = 15,
  [int]$Sample = 10,
  [string]$DataDir = ".dev-data\release",
  [string]$Route = ""
)

$ErrorActionPreference = "Stop"
$argsList = @("--data-dir", $DataDir)
if ($Route) { $argsList += @("--dev-route", $Route) }
$app = Start-Process -FilePath $Exe -ArgumentList $argsList -PassThru
Start-Sleep -Seconds $Settle

function Get-Tree([int]$rootId) {
  $all = Get-CimInstance Win32_Process
  $ids = [System.Collections.Generic.List[int]]::new()
  $ids.Add($rootId)
  $added = $true
  while ($added) {
    $added = $false
    foreach ($p in $all) {
      if ($ids.Contains([int]$p.ParentProcessId) -and -not $ids.Contains([int]$p.ProcessId)) {
        $ids.Add([int]$p.ProcessId); $added = $true
      }
    }
  }
  return $ids
}

$ids = Get-Tree $app.Id
$procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue } | Where-Object { $_ }

$cpuStart = ($procs | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
Start-Sleep -Seconds $Sample
$procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue } | Where-Object { $_ }
$cpuEnd = ($procs | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum

$rows = foreach ($p in $procs) {
  [pscustomobject]@{
    Name = $p.ProcessName
    Id = $p.Id
    WorkingSetMB = [math]::Round($p.WorkingSet64 / 1MB, 1)
    PrivateMB = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
  }
}
$rows | Sort-Object PrivateMB -Descending | Format-Table -AutoSize | Out-String | Write-Host

# Private working set (what Task Manager's "Memory" column shows), matched by PID.
$pwsTotal = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
  Where-Object { $ids.Contains([int]$_.IDProcess) } |
  Measure-Object -Property WorkingSetPrivate -Sum).Sum

$ws = ($rows | Measure-Object -Property WorkingSetMB -Sum).Sum
$priv = ($rows | Measure-Object -Property PrivateMB -Sum).Sum
$cores = [Environment]::ProcessorCount
$cpuPct = (($cpuEnd - $cpuStart) / ($Sample * 1000)) / $cores * 100

Write-Host ("processes:                 {0}" -f $rows.Count)
Write-Host ("private working set (TM):  {0:N1} MB" -f ($pwsTotal / 1MB))
Write-Host ("working set (incl shared): {0:N1} MB" -f $ws)
Write-Host ("private bytes (commit):    {0:N1} MB" -f $priv)
Write-Host ("idle CPU over {0}s:         {1:N2} % of all {2} cores ({3:N0} ms CPU)" -f $Sample, $cpuPct, $cores, ($cpuEnd - $cpuStart))

Stop-Process -Id $app.Id -ErrorAction SilentlyContinue
