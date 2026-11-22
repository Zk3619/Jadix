import UIKit

@MainActor
func jadixEnvironmentSnapshot(_ view: UIView) -> [String: Any] {
    let preferences = jadixLocalePreferences()
    let state = view.window?.windowScene?.activationState
    let lifecycle: String
    if state == .background { lifecycle = "background" }
    else if state == .foregroundActive { lifecycle = "active" }
    else if state != nil { lifecycle = "inactive" }
    else { lifecycle = UIApplication.shared.applicationState == .active ? "active" : (UIApplication.shared.applicationState == .background ? "background" : "inactive") }
    return ["platform": ["name": "ios", "os": "ios"], "locale": preferences,
        "color_scheme": view.traitCollection.userInterfaceStyle == .dark ? "dark" : "light",
        "layout_direction": view.effectiveUserInterfaceLayoutDirection == .rightToLeft ? "right-to-left" : "left-to-right",
        "high_contrast": view.traitCollection.accessibilityContrast == .high,
        "reduced_motion": UIAccessibility.isReduceMotionEnabled,
        "font_scale": min(4, max(0.5, UIFontMetrics.default.scaledValue(for: 17, compatibleWith: view.traitCollection) / 17)),
        "lifecycle": lifecycle]
}
