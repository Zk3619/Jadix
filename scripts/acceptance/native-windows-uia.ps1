param(
    [Parameter(Mandatory=$true)][long]$WindowHandle,
    [Parameter(Mandatory=$true)][string]$Output
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$WindowHandle)
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
$rows = foreach ($node in $nodes) {
    $current = $node.Current
    $rectangle = $current.BoundingRectangle
    $row = [ordered]@{
        name = $current.Name
        role = $current.ControlType.ProgrammaticName
        enabled = $current.IsEnabled
        focusable = $current.IsKeyboardFocusable
        focused = $current.HasKeyboardFocus
        offscreen = $current.IsOffscreen
        rectangle = @($rectangle.X, $rectangle.Y, $rectangle.Width, $rectangle.Height)
    }
    $pattern = $null
    if ($node.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $row.value = $pattern.Current.Value
    }
    $pattern = $null
    if ($node.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
        $row.selected = $pattern.Current.IsSelected
    }
    $row
}
[ordered]@{ nodes = @($rows) } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding UTF8
