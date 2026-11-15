param(
    [Parameter(Mandatory=$true)][long]$WindowHandle,
    [Parameter(Mandatory=$true)][string]$Output,
    [string]$Invoke
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$WindowHandle)
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
$deadline = [DateTime]::UtcNow.AddSeconds(3)
while ($nodes.Count -le 1 -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 100
    $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
}
$rows = @()
$invoked = $false
foreach ($node in $nodes) {
    $current = $node.Current
    $row = [ordered]@{
        name = $current.Name
        role = $current.ControlType.ProgrammaticName
        enabled = $current.IsEnabled
        focusable = $current.IsKeyboardFocusable
        offscreen = $current.IsOffscreen
    }
    if ($current.ControlType -eq [System.Windows.Automation.ControlType]::ProgressBar) {
        $range = $node.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).Current
        $row.range = [ordered]@{ value = $range.Value; minimum = $range.Minimum; maximum = $range.Maximum; read_only = $range.IsReadOnly }
    }
    $rows += $row
    if ($Invoke -and $current.Name -eq $Invoke -and $current.IsEnabled) {
        $pattern = $node.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        $invoked = $true
    }
}
[ordered]@{ nodes = $rows; invoked = $invoked } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding UTF8
if ($Invoke -and -not $invoked) { throw "UIA action was not found: $Invoke" }
