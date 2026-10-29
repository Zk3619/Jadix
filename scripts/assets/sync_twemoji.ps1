param(
    # all = 一次下载解压，同时写入 72x72 PNG 与 svg（推荐）；png / svg = 仅同步一种（仍只解压一次包，但只复制对应目录）
    [string]$AssetKind = "all",
    [string]$Repo = "jdecked/twemoji",
    [string]$Version = "v16.0.1",
    [string]$TargetDir = ""
)

$ErrorActionPreference = "Stop"

if ($AssetKind -ne "png" -and $AssetKind -ne "svg" -and $AssetKind -ne "all") {
    throw "AssetKind must be 'all', 'png', or 'svg', got: $AssetKind"
}

if ($AssetKind -eq "all" -and -not [string]::IsNullOrWhiteSpace($TargetDir)) {
    throw "When -AssetKind is 'all', omit -TargetDir (uses assets/twemoji/72x72 and assets/twemoji/svg)."
}

# 不依赖 $PSScriptRoot（部分宿主为空），统一从脚本文件路径推导仓库根目录
$scriptDir = [System.IO.Path]::GetDirectoryName($MyInvocation.MyCommand.Path)
if ([string]::IsNullOrWhiteSpace($scriptDir)) {
    if (-not [string]::IsNullOrWhiteSpace($PSScriptRoot)) {
        $scriptDir = $PSScriptRoot
    } else {
        throw "Cannot resolve script directory (empty MyInvocation path and PSScriptRoot)."
    }
}
$repoRoot = Resolve-Path (Join-Path $scriptDir "../..")

if ([string]::IsNullOrWhiteSpace($Repo)) {
    $Repo = "jdecked/twemoji"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v16.0.1"
}

if ($AssetKind -ne "all") {
    if ([string]::IsNullOrWhiteSpace($TargetDir)) {
        $TargetDir = if ($AssetKind -eq "svg") { "assets/twemoji/svg" } else { "assets/twemoji/72x72" }
    }
    $targetPath = Join-Path $repoRoot $TargetDir
}

$tempRoot = Join-Path $repoRoot ".tmp"
$repoSlug = $Repo.Replace("/", "-")
$zipPath = Join-Path $tempRoot "$repoSlug-$Version.zip"
$extractPath = Join-Path $tempRoot "$repoSlug-$Version-extract"
$downloadUrl = "https://github.com/$Repo/archive/refs/tags/$Version.zip"

function Copy-TwemojiAssetFolder {
    param(
        [Parameter(Mandatory)][string]$TopFullName,
        [Parameter(Mandatory)][string]$AssetsSub,
        [Parameter(Mandatory)][string]$Filter,
        [Parameter(Mandatory)][string]$Destination
    )
    $srcPath = Join-Path $TopFullName "assets\$AssetsSub"
    if (-not (Test-Path $srcPath)) {
        throw "Twemoji source path not found: $srcPath (check Repo/Version and archive layout)"
    }
    Write-Host "Syncing $Filter from $srcPath to $Destination"
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Get-ChildItem -Path $Destination -File -Filter $Filter -ErrorAction SilentlyContinue | Remove-Item -Force
    Get-ChildItem -Path $srcPath -File -Filter $Filter | Copy-Item -Destination $Destination -Force
    return @(Get-ChildItem -Path $Destination -File -Filter $Filter).Count
}

New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
if ($AssetKind -ne "all") {
    New-Item -ItemType Directory -Path $targetPath -Force | Out-Null
}

Write-Host "Repo=$Repo Version=$Version AssetKind=$AssetKind"
Write-Host "Downloading $downloadUrl"
Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath

if (Test-Path $extractPath) {
    Remove-Item -Path $extractPath -Recurse -Force
}

Write-Host "Extracting archive"
Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force

$top = Get-ChildItem -Path $extractPath -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $top) {
    throw "No top-level directory under extract path: $extractPath"
}

if ($AssetKind -eq "all") {
    $targetPng = Join-Path $repoRoot "assets/twemoji/72x72"
    $targetSvg = Join-Path $repoRoot "assets/twemoji/svg"
    $nPng = Copy-TwemojiAssetFolder -TopFullName $top.FullName -AssetsSub "72x72" -Filter "*.png" -Destination $targetPng
    $nSvg = Copy-TwemojiAssetFolder -TopFullName $top.FullName -AssetsSub "svg" -Filter "*.svg" -Destination $targetSvg
    Write-Host "Done. Synced $nPng png and $nSvg svg files (one download / one extract)."
} elseif ($AssetKind -eq "png") {
    $n = Copy-TwemojiAssetFolder -TopFullName $top.FullName -AssetsSub "72x72" -Filter "*.png" -Destination $targetPath
    Write-Host "Done. Synced $n png files."
} else {
    $n = Copy-TwemojiAssetFolder -TopFullName $top.FullName -AssetsSub "svg" -Filter "*.svg" -Destination $targetPath
    Write-Host "Done. Synced $n svg files."
}

Write-Host "Cleaning temp files"
Remove-Item -Path $zipPath -Force -ErrorAction SilentlyContinue
Remove-Item -Path $extractPath -Recurse -Force -ErrorAction SilentlyContinue
