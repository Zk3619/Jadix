param(
    [Parameter(Mandatory)][string]$Serial,
    [string]$Apk,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist/android-verification'),
    [ValidateRange(3, 60)][int]$WaitSeconds = 45
)
$ErrorActionPreference = 'Stop'
$adb = Join-Path $env:ANDROID_HOME 'platform-tools/adb.exe'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
$deviceState = & $adb -s $Serial get-state 2>$null
if ($LASTEXITCODE -ne 0 -or $deviceState -ne 'device') { throw "Android device '$Serial' is not connected and authorized." }
if ($Apk) {
    & $adb -s $Serial install -r $Apk
    if ($LASTEXITCODE -ne 0) { throw 'APK installation failed.' }
}
& $adb -s $Serial shell am force-stop org.jadix.gallery
$startedAt = (& $adb -s $Serial shell date '+%s') + '.000'
if ($LASTEXITCODE -ne 0) { throw 'Cannot read the Android device clock.' }
& $adb -s $Serial shell am start -W -n org.jadix.gallery/android.app.NativeActivity |
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'activity-start.log')
if ($LASTEXITCODE -ne 0) { throw 'Gallery activity could not start.' }
Start-Sleep -Seconds $WaitSeconds
& $adb -s $Serial logcat -d -T $startedAt -s RustStdoutStderr AndroidRuntime libc |
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'startup.log')
if ($LASTEXITCODE -ne 0) { throw 'Android startup log capture failed.' }
$appProcess = & $adb -s $Serial shell pidof org.jadix.gallery
if ($LASTEXITCODE -ne 0 -or !$appProcess) { throw 'Gallery exited during startup. See startup.log.' }
$activity = & $adb -s $Serial shell dumpsys activity activities
$activity | Select-String 'mResumedActivity|topResumedActivity' |
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'foreground.log')
if (!(($activity | Select-String '(mResumedActivity|topResumedActivity).*org\.jadix\.gallery'))) {
    throw 'Gallery is alive but is not the resumed foreground activity.'
}
$screenshot = Join-Path $OutputDirectory 'gallery.png'
$capture = [Diagnostics.ProcessStartInfo]::new($adb)
$capture.UseShellExecute = $false
$capture.CreateNoWindow = $true
$capture.RedirectStandardOutput = $true
foreach ($argument in @('-s', $Serial, 'exec-out', 'screencap', '-p')) { $capture.ArgumentList.Add($argument) }
$process = [Diagnostics.Process]::Start($capture)
$stream = [IO.File]::Create($screenshot)
try { $process.StandardOutput.BaseStream.CopyTo($stream) } finally { $stream.Dispose() }
$process.WaitForExit()
$captureExitCode = $process.ExitCode
$process.Dispose()
if ($captureExitCode -ne 0) { throw 'Android screenshot capture failed.' }
Add-Type -AssemblyName System.Drawing
$bitmap = [Drawing.Bitmap]::new($screenshot)
try {
    $colors = [Collections.Generic.HashSet[int]]::new()
    for ($row = 48; $row -lt $bitmap.Height - 48; $row += 8) {
        for ($column = 8; $column -lt $bitmap.Width - 8; $column += 8) {
            [void]$colors.Add($bitmap.GetPixel($column, $row).ToArgb())
        }
    }
    if ($colors.Count -lt 24) { throw 'Gallery foreground is blank or nearly uniform. Inspect gallery.png.' }
    "Gallery process $appProcess is alive after ${WaitSeconds}s; captured $($bitmap.Width)x$($bitmap.Height), $($colors.Count) sampled colors."
} finally { $bitmap.Dispose() }
