import Foundation

func jadixLocalePreferences() -> [String: Any] {
    let locale = Locale.autoupdatingCurrent
    func language(_ value: String) -> String {
        String(value.split(separator: "@").first ?? "en-US").replacingOccurrences(of: "_", with: "-")
    }
    var languages: [String] = []
    for value in Locale.preferredLanguages.prefix(16).map(language) {
        if !languages.contains(where: { $0.caseInsensitiveCompare(value) == .orderedSame }) { languages.append(value) }
    }
    if languages.isEmpty { languages = [language(locale.identifier)] }
    var preferences: [String: Any] = [
        "locale": language(locale.identifier), "languages": languages,
        "time_zone": ["kind": "iana", "id": TimeZone.autoupdatingCurrent.identifier],
        "first_day_of_week": (Calendar.autoupdatingCurrent.firstWeekday + 5) % 7 + 1
    ]
    let pattern = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: locale) ?? ""
    var quoted = false
    for character in pattern {
        if character == "'" { quoted.toggle(); continue }
        if !quoted, let cycle = ["K": "h11", "h": "h12", "H": "h23", "k": "h24"][String(character)] {
            preferences["hour_cycle"] = cycle; break
        }
    }
    if #available(iOS 16, macOS 13, *) {
    switch locale.measurementSystem {
    case .metric: preferences["measurement_system"] = "metric"
    case .us: preferences["measurement_system"] = "us"
    case .uk: preferences["measurement_system"] = "uk"
    default: break
    }
    } else { preferences["measurement_system"] = locale.usesMetricSystem ? "metric" : "us" }
    if let temperature = UserDefaults.standard.string(forKey: "AppleTemperatureUnit") {
        if temperature == "Celsius" { preferences["temperature_unit"] = "celsius" }
        if temperature == "Fahrenheit" { preferences["temperature_unit"] = "fahrenheit" }
    }
    return preferences
}
