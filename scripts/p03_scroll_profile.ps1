# P.0.3: Gallery release scroll profile (~10s synthetic wheel). See size_and_memory.md §7.5.
$ErrorActionPreference = "Stop"
$root = "d:\Projects\Rust\rust-native-ui"
$exe = Join-Path $root "target\release\gallery.exe"
$logFile = Join-Path $root "crates\rui\tmp_p03_scroll_profile.log"
Remove-Item $logFile -ErrorAction SilentlyContinue

$env:RUST_LOG = "warn"
$env:RUI_RENDER_PROFILE = "1"

$proc = Start-Process -FilePath $exe -RedirectStandardError $logFile -PassThru -WindowStyle Normal
Start-Sleep -Seconds 4
$gp = Get-Process -Id $proc.Id -ErrorAction Stop
if ($gp.HasExited) {
    Get-Content $logFile -ErrorAction SilentlyContinue
    throw "Gallery exited early"
}

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class P03Win32 {
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
"@

$hwnd = $gp.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $gp.Refresh()
    $hwnd = $gp.MainWindowHandle
}
if ($hwnd -eq [IntPtr]::Zero) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    throw "Gallery MainWindowHandle is zero"
}
[void][P03Win32]::SetForegroundWindow($hwnd)

$end = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $end) {
    $wParam = [IntPtr](([int]-120) -shl 16)
    [void][P03Win32]::PostMessage($hwnd, 0x020A, $wParam, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 16
}

Start-Sleep -Seconds 2
if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
}

Write-Output "LOG=$logFile"
Get-Content $logFile | Select-String "render profile"
