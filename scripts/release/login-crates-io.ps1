[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') {
    throw 'This helper requires the Windows Credential Manager provider.'
}
$previousProvider = $env:CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER
$secret = $null
$pointer = [IntPtr]::Zero
try {
    $env:CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER = 'cargo:wincred'
    $secret = Read-Host 'crates.io API token (hidden; never paste it into chat)' -AsSecureString
    if ($secret.Length -eq 0) { throw 'No token supplied.' }
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
    [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) | & cargo login --registry crates-io
    if ($LASTEXITCODE -ne 0) { throw "cargo login failed: $LASTEXITCODE" }
    Write-Host 'Credential stored by cargo:wincred. Upload permissions still require registry validation.'
}
finally {
    if ($pointer -ne [IntPtr]::Zero) {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
    }
    if ($null -ne $secret) { $secret.Dispose() }
    if ($null -eq $previousProvider) {
        Remove-Item Env:CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER = $previousProvider
    }
}
