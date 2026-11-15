param(
    [string]$Sdk = $env:OHOS_NDK_HOME,
    [Parameter(Mandatory = $true)][string]$Output
)

$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$receipt = [IO.Path]::GetFullPath($Output)
$log = [IO.Path]::ChangeExtension($receipt, '.log')
if ((Test-Path -LiteralPath $receipt) -or (Test-Path -LiteralPath $log)) {
    throw 'Receipt or log already exists; select a new output path.'
}
if (-not (Test-Path -LiteralPath (Split-Path -Parent $receipt) -PathType Container)) {
    throw 'Output parent directory must exist.'
}
$sdkRoot = (Resolve-Path -LiteralPath $Sdk).Path
$headers = Join-Path $sdkRoot 'native/sysroot/usr/include'
if (-not (Test-Path -LiteralPath (Join-Path $headers 'arkui/native_dialog.h'))) {
    throw 'Select an installed OpenHarmony SDK with ArkUI headers.'
}
$programFiles = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFiles 'Microsoft Visual Studio/Installer/vswhere.exe'
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $visualStudio) { throw 'MSVC x64 build tools were not found.' }
$toolsVersion = (Get-Content -LiteralPath (Join-Path $visualStudio 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt') -Raw).Trim()
$tools = Join-Path $visualStudio "VC/Tools/MSVC/$toolsVersion"
$kits = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots').KitsRoot10
$kitVersion = Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory |
    Where-Object { (Test-Path -LiteralPath (Join-Path $_.FullName 'ucrt/stdio.h')) -and (Test-Path -LiteralPath (Join-Path $kits "Lib/$($_.Name)/um/x64/kernel32.lib")) } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1 -ExpandProperty Name
if (-not $kitVersion) { throw 'Windows SDK x64 headers and libraries were not found.' }
$build = Join-Path $repository 'target/native-ohos-select-contract'
New-Item -ItemType Directory -Force -Path $build | Out-Null
$source = Join-Path $repository 'crates/jadix_native/platforms/ohos/native.cpp'
$contract = Join-Path $repository 'crates/jadix_native/tests/ohos_select_contract.cpp'
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
$contractHash = (Get-FileHash -LiteralPath $contract -Algorithm SHA256).Hash.ToLowerInvariant()
$object = Join-Path $build 'contract.obj'
$executable = Join-Path $build 'contract.exe'
$compiler = Join-Path $tools 'bin/Hostx64/x64/cl.exe'
$linker = Join-Path $tools 'bin/Hostx64/x64/link.exe'

function Invoke-ContractTool([string]$Program, [string[]]$Arguments) {
    Add-Content -LiteralPath $log -Encoding UTF8 -Value ($Program + ' ' + ($Arguments -join ' '))
    $lines = & $Program @Arguments 2>&1
    $code = $LASTEXITCODE
    $lines | ForEach-Object { Add-Content -LiteralPath $log -Encoding UTF8 -Value $_.ToString(); Write-Host $_ }
    if ($code -ne 0) { throw "Contract command failed with exit code $code. See $log" }
    return $lines
}

$compileArguments = @('/nologo', '/std:c++20', '/EHsc', '/utf-8', '/W4', '/WX', '/Gy', '/Zc:inline',
    '/I', (Join-Path $tools 'include'), '/I', (Join-Path $kits "Include/$kitVersion/ucrt"),
    '/external:I', $headers, '/external:W0', '/c', $contract, "/Fo$object")
Invoke-ContractTool $compiler $compileArguments | Out-Null
$linkArguments = @('/nologo', '/OPT:REF', "/OUT:$executable", $object,
    "/LIBPATH:$(Join-Path $tools 'lib/x64')", "/LIBPATH:$(Join-Path $kits "Lib/$kitVersion/ucrt/x64")",
    "/LIBPATH:$(Join-Path $kits "Lib/$kitVersion/um/x64")")
Invoke-ContractTool $linker $linkArguments | Out-Null
$result = Invoke-ContractTool $executable @()
if (($result -join "\n") -notmatch '6 production C\+\+ Select callback scenarios passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '1 production C\+\+ Radio callback scenario passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '3 production C\+\+ unsupported accessibility role scenarios passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '2 production C\+\+ toggle accessibility state scenarios passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '7 production C\+\+ focus lifecycle controls passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '1 production C\+\+ Submit callback scenario passed; ArkUI runtime acceptance=false' -or
    ($result -join "\n") -notmatch '1 production C\+\+ accessibility provider focus, content notification and action lifecycle passed; ArkUI runtime acceptance=false') {
    throw 'The contract executable did not report all Select, Radio, accessibility role and focus lifecycle scenarios.'
}
if ($sourceHash -ne (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -or
    $contractHash -ne (Get-FileHash -LiteralPath $contract -Algorithm SHA256).Hash.ToLowerInvariant()) {
    throw 'Source changed during verification; no passing receipt is written.'
}
[ordered]@{
    schema = 1
    recorded_at = [DateTime]::UtcNow.ToString('o')
    authority = 'production-cpp-with-sdk-headers-and-mocked-arkui'
    source = $source
    source_sha256 = $sourceHash
    contract = $contract
    contract_sha256 = $contractHash
    source_unchanged = $true
    sdk = $sdkRoot
    compiler = $compiler
    compiler_sha256 = (Get-FileHash -LiteralPath $compiler -Algorithm SHA256).Hash.ToLowerInvariant()
    windows_sdk = $kitVersion
    executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    scenarios = @('theme-and-retention', 'keyboard-navigation', 'selection-and-identity', 'dismissal-and-disposal', 'failed-close-is-terminal', 'partial-creation-is-disposed', 'radio-node-state-navigation-cleanup-accessibility-role', 'tree-tree-item-radio-group-role-reset', 'checkbox-switch-checked-accessibility-state', 'focus-blur-seven-controls-and-disposal', 'input-submit-textarea-newline-and-disposal', 'xcomponent-accessibility-provider-click-stale-id-and-disposal')
    status = 'pass'
    runtime_acceptance = $false
    accessibility_runtime_acceptance = $false
    log = $log
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receipt -Encoding UTF8
Write-Host "Select C++ contract receipt: $receipt"
