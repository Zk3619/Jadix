param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Push-Location $root
try {
    $options = @('metadata', '--no-deps', '--format-version', '1')
    if ($Offline) { $options += '--offline' }
    $metadata = & cargo @options | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Workspace metadata failed' }
    $packages = @{}
    foreach ($package in $metadata.packages) { $packages[$package.name] = $package }
    $lower = @('jadix_core', 'jadix_layout', 'jadix_renderer', 'jadix_animation', 'jadix_theme', 'jadix_window_materials', 'jadix_document', 'jadix_hot_reload', 'jadix_lsp', 'jadix_media', 'jadix_net', 'jadix_tasks', 'jadix_tokio', 'jadix_style_tokens')
    $forbidden = @('jadix', 'jadix_components', 'jadix_dnd', 'jadix_lucide', 'jadix_macros', 'jadix_winit')
    foreach ($name in $lower) {
        if (-not $packages.ContainsKey($name)) { throw "Missing lower package: $name" }
        $pending = [Collections.Generic.Queue[string]]::new()
        $visited = [Collections.Generic.HashSet[string]]::new()
        $pending.Enqueue($name)
        while ($pending.Count) {
            $current = $pending.Dequeue()
            if (-not $visited.Add($current)) { continue }
            foreach ($dependency in $packages[$current].dependencies) {
                if ($dependency.kind -eq 'dev') { continue }
                if ($dependency.name -in $forbidden) { throw "Reverse dependency: $name -> $current -> $($dependency.name)" }
                if ($packages.ContainsKey($dependency.name)) { $pending.Enqueue($dependency.name) }
            }
        }
        Write-Host "Lower boundary passed: $name ($($visited.Count) workspace packages)"
    }
    foreach ($dependency in $packages['jadix'].dependencies) {
        if ($dependency.kind -ne 'dev' -and $dependency.name -in @('jadix_components', 'jadix_dnd', 'jadix_lucide', 'jadix_hot_reload')) {
            throw "Framework depends on product: $($dependency.name)"
        }
    }
    foreach ($name in @('jadix_components', 'jadix_dnd', 'jadix_lucide')) {
        foreach ($dependency in $packages[$name].dependencies) {
            if ($dependency.kind -ne 'dev' -and $dependency.name -eq 'jadix_macros' -and -not $dependency.optional) {
                throw "Mandatory parser dependency: $name"
            }
        }
    }
    $animation = $packages['jadix_animation'].features
    foreach ($name in @('animation-base', 'animation-ex', 'animation-path')) {
        if (-not $animation.PSObject.Properties[$name]) { throw "Missing animation tier: $name" }
    }
    if ('animation-base' -notin $animation.'animation-ex' -or 'animation-ex' -notin $animation.'animation-path') {
        throw 'Animation tiers must be cumulative'
    }
    if ($packages.ContainsKey('jadix_text') -or $packages.ContainsKey('jadix_lucide_data')) {
        throw 'Retired package remains in workspace'
    }
    Write-Host 'Runtime dependency directions, retired names and animation tiers passed.'
} finally { Pop-Location }
