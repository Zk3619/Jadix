import Foundation

/// Both Apple hosts publish the shared root address through the same activity lifecycle.
/// Relative addresses remain local; only absolute URLs can become Handoff activities.
final class JadixHandoff {
    private var address: String?
    private var activity: NSUserActivity?
    static var activityType: String? { Bundle.main.bundleIdentifier.map { "\($0).url" } }
    static var declared: Bool {
        guard let type = activityType else { return false }
        return (Bundle.main.object(forInfoDictionaryKey: "NSUserActivityTypes") as? [String] ?? []).contains(type)
    }
    static func continuedURL(_ activity: NSUserActivity) -> URL? {
        let url: URL?
        if activity.activityType == NSUserActivityTypeBrowsingWeb { url = activity.webpageURL }
        else if declared, activity.activityType == activityType, let value = activity.userInfo?["url"] as? String {
            url = URL(string: value)
        } else { return nil }
        guard let url, safeURL(url) else { return nil }; return url
    }
    private static func safeURL(_ url: URL) -> Bool {
        let value = url.absoluteString
        guard !value.isEmpty, value.utf8.count <= 32768,
              !value.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.union(.controlCharacters).contains($0) }),
              let scheme = url.scheme?.lowercased(),
              !["file", "javascript", "vbscript", "data", "blob", "about", "asset"].contains(scheme),
              url.user == nil, url.password == nil else { return false }
        return !["http", "https"].contains(scheme) || url.host?.isEmpty == false
    }
    func setAddress(_ value: String?) {
        guard value != address else { return }
        activity?.invalidate(); activity = nil; address = value
    }
    func refresh(enabled: Bool, active: Bool) {
        guard enabled, active else { activity?.resignCurrent(); return }
        if let activity { activity.becomeCurrent(); return }
        guard let address, let url = URL(string: address), Self.safeURL(url) else { return }
        let next: NSUserActivity
        if ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
            next = NSUserActivity(activityType: NSUserActivityTypeBrowsingWeb); next.webpageURL = url
        } else {
            guard Self.declared, let type = Self.activityType else { return }
            next = NSUserActivity(activityType: type); next.userInfo = ["url": address]; next.requiredUserInfoKeys = ["url"]
        }
        next.isEligibleForHandoff = true; next.isEligibleForSearch = false; next.isEligibleForPublicIndexing = false
        activity = next; next.becomeCurrent()
    }
    func clear() { activity?.invalidate(); activity = nil; address = nil }
    deinit { activity?.invalidate() }
}
