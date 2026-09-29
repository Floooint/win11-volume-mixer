# 对照 docs/readme.md“MVP 验收标准”测量 release 构建：
# 隐藏时内存、显示时内存、后台空闲 CPU、打开窗口耗时。
# 用法：powershell -File measure-acceptance.ps1 -Policy silent|resident|smart
# 会结束正在运行的本程序。打开窗口通过再次启动 exe（单实例会唤起已有窗口）实现。
param([string]$Policy = "silent", [int]$IdleSeconds = 60, [int]$Opens = 4)

$exe = Join-Path $PSScriptRoot "..\src-tauri\target\release\win11-volume-mixer.exe"
$log = Join-Path $env:TEMP "volume-mixer-acceptance.log"
Get-Process win11-volume-mixer -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

function Get-Tree([int]$root) {
    $all = Get-CimInstance Win32_Process
    $ids = @($root)
    $queue = @($root)
    while ($queue.Count -gt 0) {
        $next = @()
        foreach ($p in $queue) {
            $next += $all | Where-Object { $_.ParentProcessId -eq $p } | ForEach-Object { $_.ProcessId }
        }
        $ids += $next
        $queue = $next
    }
    return $ids
}

function Measure-Memory([int]$root, [string]$label) {
    $procs = Get-Tree $root | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
    $private = ($procs | Measure-Object -Property PrivateMemorySize64 -Sum).Sum / 1MB
    $ws = ($procs | Measure-Object -Property WorkingSet64 -Sum).Sum / 1MB
    "{0}：进程数 {1}，私有内存 {2:N1} MB，工作集 {3:N1} MB" -f $label, $procs.Count, $private, $ws
}

$env:VOLUME_MIXER_WINDOW = $Policy
$proc = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden -RedirectStandardError $log
Start-Sleep -Seconds 8
"运行模式：$Policy"
Measure-Memory $proc.Id "启动后（未打开窗口）"

# 空闲 CPU：窗口从未打开，只有托盘和音频线程。
$before = (Get-Process -Id $proc.Id).TotalProcessorTime
Start-Sleep -Seconds $IdleSeconds
$after = (Get-Process -Id $proc.Id).TotalProcessorTime
"空闲 {0} 秒 CPU 时间增加 {1:N0} ms（{2:P3}）" -f $IdleSeconds, ($after - $before).TotalMilliseconds, (($after - $before).TotalSeconds / $IdleSeconds / [Environment]::ProcessorCount)

# 反复打开：每次打开后临时显示一个窗口抢走焦点，主窗口因失焦隐藏（静默模式下随即释放）。
Add-Type -AssemblyName System.Windows.Forms
for ($i = 1; $i -le $Opens; $i++) {
    Start-Process -FilePath $exe -WindowStyle Hidden | Out-Null
    Start-Sleep -Seconds 3
    if ($i -eq 1) { Measure-Memory $proc.Id "窗口显示时" }
    $form = New-Object System.Windows.Forms.Form
    $form.Text = "measure"; $form.Width = 200; $form.Height = 100
    $form.Show(); $form.Activate()
    Start-Sleep -Seconds 2
    $form.Close()
    if ($i -eq 1) { Measure-Memory $proc.Id "窗口隐藏后" }
}

Start-Sleep -Seconds 1
Get-Tree $proc.Id | Sort-Object -Descending | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 500
"打开耗时（程序日志）："
Get-Content $log -Encoding UTF8 | Where-Object { $_ -match "打开耗时" }
