function Read-GalleryOhosSigningConfig {
    param(
        [Parameter(Mandatory)][string]$Path,
        [string]$Name = 'default',
        [Parameter(Mandatory)][string]$DevecoHome
    )
    $configurationPath = (Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
    $node = Join-Path $DevecoHome 'tools/node/node.exe'
    $json5 = Join-Path $DevecoHome 'tools/hvigor/hvigor-ohos-plugin/node_modules/json5'
    $configurationJson = & $node -e 'const fs=require("fs"); const json5=require(process.argv[1]); process.stdout.write(JSON.stringify(json5.parse(fs.readFileSync(process.argv[2], "utf8"))));' $json5 $configurationPath 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the DevEco JSON5 signing configuration.' }
    $configuration = $configurationJson | ConvertFrom-Json -ErrorAction Stop
    $matches = @($configuration.app.signingConfigs | Where-Object { $_.name -eq $Name })
    if ($matches.Count -ne 1) { throw "Expected exactly one signing configuration named '$Name'." }
    $signing = $matches[0]
    foreach ($property in @('profile', 'certpath', 'storeFile')) {
        $materialPath = $signing.material.$property
        if (!$materialPath) { throw "Signing material '$property' is missing." }
        if (![IO.Path]::IsPathRooted($materialPath)) {
            $materialPath = Join-Path (Split-Path -Parent $configurationPath) $materialPath
        }
        $signing.material.$property = (Resolve-Path -LiteralPath $materialPath -ErrorAction Stop).Path
    }
    Add-Type -AssemblyName System.Security.Cryptography.Pkcs
    $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new()
    $cms.Decode([IO.File]::ReadAllBytes($signing.material.profile))
    $profile = [Text.Encoding]::UTF8.GetString($cms.ContentInfo.Content) | ConvertFrom-Json -ErrorAction Stop
    if ($profile.'bundle-info'.'bundle-name' -ne 'org.jadix.gallery') {
        throw 'The signing profile must authorize bundle org.jadix.gallery. Generate a Gallery profile in DevEco Studio.'
    }
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    if (!$profile.validity.'not-before' -or !$profile.validity.'not-after' -or
        $now -lt $profile.validity.'not-before' -or $now -ge $profile.validity.'not-after') {
        throw 'The Gallery signing profile is expired or not yet valid. Renew it in DevEco Studio.'
    }
    $deviceIds = @($profile.'debug-info'.'device-ids' | Where-Object { ![string]::IsNullOrWhiteSpace($_) })
    if ($profile.type -eq 'debug' -and $deviceIds.Count -eq 0) {
        throw 'The debug signing profile does not authorize a device. Add the target UDID in DevEco Studio.'
    }
    return $signing
}
