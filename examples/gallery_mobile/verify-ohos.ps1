param(
    [Parameter(Mandatory)][string]$Serial,
    [string]$Hap,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist/ohos-verification'),
    [ValidateRange(3, 60)][int]$WaitSeconds = 45
)
$ErrorActionPreference = 'Stop'
if (!$env:DEVECO_SDK_HOME) { throw 'Set DEVECO_SDK_HOME to the DevEco SDK directory.' }
$hdc = Join-Path $env:DEVECO_SDK_HOME 'default/openharmony/toolchains/hdc.exe'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
$targets = & $hdc list targets
if ($LASTEXITCODE -ne 0 -or !($targets | Where-Object { $_.Trim() -eq $Serial })) {
    throw "OHOS device '$Serial' is not connected."
}
if ($Hap) {
    $installation = & $hdc -t $Serial install $Hap
    $installation | Set-Content -LiteralPath (Join-Path $OutputDirectory 'install.log')
    if ($LASTEXITCODE -ne 0 -or ($installation -join "`n") -notmatch 'install bundle successfully') {
        throw 'HAP installation failed. See install.log for the device verdict.'
    }
}
& $hdc -t $Serial shell 'aa force-stop org.jadix.gallery' | Out-Null
$started = & $hdc -t $Serial shell 'aa start -a EntryAbility -b org.jadix.gallery'
$started | Set-Content -LiteralPath (Join-Path $OutputDirectory 'ability-start.log')
if ($LASTEXITCODE -ne 0 -or ($started -join "`n") -notmatch 'start ability successfully') {
    throw 'Gallery ability could not start. See ability-start.log.'
}
Start-Sleep -Seconds $WaitSeconds
$appProcess = & $hdc -t $Serial shell 'pidof org.jadix.gallery'
$alive = $LASTEXITCODE -eq 0 -and ($appProcess -join '').Trim() -match '^\d+(\s+\d+)*$'
if ($alive) {
    & $hdc -t $Serial shell "hilog -x -P $(($appProcess -join '').Trim().Split(' ')[0])" |
        Set-Content -LiteralPath (Join-Path $OutputDirectory 'startup.log')
} else {
    & $hdc -t $Serial shell 'hilog -x' |
        Set-Content -LiteralPath (Join-Path $OutputDirectory 'startup.log')
    throw 'Gallery exited during startup. See startup.log and the device native crash report.'
}
$capture = & $hdc -t $Serial shell 'snapshot_display -f /data/local/tmp/jadix-gallery-verification.jpeg'
$capture | Set-Content -LiteralPath (Join-Path $OutputDirectory 'screenshot.log')
if ($LASTEXITCODE -ne 0 -or ($capture -join "`n") -notmatch 'success: snapshot display') {
    throw 'OHOS screenshot capture failed. See screenshot.log.'
}
$screenshot = Join-Path $OutputDirectory 'gallery.jpeg'
& $hdc -t $Serial file recv /data/local/tmp/jadix-gallery-verification.jpeg $screenshot
if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $screenshot)) { throw 'OHOS screenshot transfer failed.' }
"Gallery process $appProcess is alive after ${WaitSeconds}s. Inspect gallery.jpeg for actual Gallery content and test interactions."
