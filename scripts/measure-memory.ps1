# 测量窗口隐藏策略：常驻内存（主进程 + WebView2 子进程）。
# 用法：powershell -File measure-memory.ps1 -Policy keep|destroy
param([string]$Policy = "keep", [int]$Seconds = 12)

$exe = Join-Path $PSScriptRoot "..\src-tauri\target\release\win11-volume-mixer.exe"
Get-Process win11-volume-mixer -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

$env:VOLUME_MIXER_WINDOW = $Policy
$proc = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds $Seconds

# 递归收集子进程（WebView2 的 browser / renderer / gpu 等）
function Get-Tree([int]$root) {
    $all = Get-CimInstance Win32_Process
    $ids = @($root)
    $queue = @($root)
    while ($queue.Count -gt 0) {
        $next = @()
        foreach ($p in $queue) {
            $children = $all | Where-Object { $_.ParentProcessId -eq $p } | ForEach-Object { $_.ProcessId }
            $next += $children
        }
        $ids += $next
        $queue = $next
    }
    return $ids
}

$ids = Get-Tree $proc.Id
$procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
$main = ($procs | Where-Object { $_.Id -eq $proc.Id }).WorkingSet64 / 1MB
$total = ($procs | Measure-Object -Property WorkingSet64 -Sum).Sum / 1MB
$private = ($procs | Measure-Object -Property PrivateMemorySize64 -Sum).Sum / 1MB

"{0}: 进程数 {1}，主进程 {2:N1} MB，合计工作集 {3:N1} MB，合计私有内存 {4:N1} MB" -f $Policy, $procs.Count, $main, $total, $private

$procs | Sort-Object Id -Descending | Stop-Process -Force -ErrorAction SilentlyContinue
