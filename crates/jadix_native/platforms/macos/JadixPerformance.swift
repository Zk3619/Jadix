import AppKit
import QuartzCore

// CADisplayLink retains its target. This proxy keeps the surface weak so that
// closing or releasing a surface always releases its display link as well.
@MainActor
final class JadixFrameTarget: NSObject {
    weak var host: JadixNativeView?
    init(_ host: JadixNativeView) { self.host = host }
    @objc func tick(_ link: CADisplayLink) { host?.performanceTick(link) }
}

@MainActor
final class JadixPerformancePanel: NSView {
    var sample: [String: Any] = [:] { didSet { needsDisplay = true; updateAccessibility() } }
    var resized: (() -> Void)?
    private var collapsed = false
    private let toggle = NSButton(title: "−", target: nil, action: nil)
    override var isFlipped: Bool { true }
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = 8
        toggle.bezelStyle = .inline
        toggle.target = self; toggle.action = #selector(collapse)
        toggle.setAccessibilityLabel("Collapse performance panel")
        addSubview(toggle)
        setAccessibilityElement(true); setAccessibilityRole(.group)
        setAccessibilityLabel("Performance")
        setFrameSize(NSSize(width: 288, height: 96))
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func layout() {
        super.layout()
        toggle.frame = NSRect(x: bounds.width - 21, y: 2, width: 18, height: 18)
    }
    @objc private func collapse() {
        collapsed.toggle(); toggle.title = collapsed ? "+" : "−"
        toggle.setAccessibilityLabel(collapsed ? "Expand performance panel" : "Collapse performance panel")
        setFrameSize(collapsed ? NSSize(width: 76, height: 24) : NSSize(width: 288, height: 96))
        resized?(); needsDisplay = true
    }
    private func number(_ value: Any?, suffix: String = "", digits: Int = 1) -> String {
        guard let number = value as? NSNumber, number.doubleValue.isFinite else { return "—" }
        return String(format: "%.*f", digits, number.doubleValue) + suffix
    }
    private func lines() -> [String] {
        let frame = sample["frame"] as? [String: Any] ?? [:]
        let js = sample["js"] as? [String: Any] ?? [:]
        let core = sample["core"] as? [String: Any] ?? [:]
        let process = sample["process"] as? [String: Any] ?? [:]
        let heap = sample["heap"] as? [String: Any] ?? [:]
        let resident = (process["working_set"] as? NSNumber).map { $0.doubleValue / 1048576 }
        let bytes = (heap["allocatedBytes"] as? NSNumber).map { $0.doubleValue / 1048576 }
        return [
            "\(number(sample["fps"], digits: 0)) fps  \(number(sample["refreshRate"], suffix: " Hz", digits: 0))",
            "Frame \(number(frame["meanMs"]))  p95 \(number(frame["p95Ms"])) ms",
            "JS \(number(js["meanMs"]))  Core \(number(core["meanMs"])) ms  CPU \(number(process["process_percent"], suffix: "%"))",
            "RSS \(number(resident, suffix: "M"))  JS heap \(number(bytes, suffix: "M")) / \(number(heap["objects"], digits: 0)) objects",
            "Nodes \(number(sample["liveNodes"], digits: 0))  Retired \(number(sample["retiredNodes"], digits: 0))"
        ]
    }
    private func updateAccessibility() { setAccessibilityValue(lines().joined(separator: ", ")) }
    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.withAlphaComponent(0.94).setFill(); bounds.fill()
        let font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor.labelColor]
        let text = lines()
        let count = collapsed ? 1 : text.count
        for index in 0..<count {
            let line = collapsed ? "\(number(sample["fps"], digits: 0)) fps" : text[index]
            (line as NSString).draw(at: NSPoint(x: 7, y: CGFloat(5 + index * 13)), withAttributes: attributes)
        }
        guard !collapsed else { return }
        let frames = Array((sample["framesMs"] as? [Double] ?? []).suffix(120))
        let chart = NSRect(x: 7, y: 73, width: bounds.width - 14, height: 17)
        let hz = (sample["refreshRate"] as? Double) ?? 60
        let budget = 1000 / max(1, hz)
        let ceiling = max(budget * 2, max(frames.max() ?? 0.0, 1))
        let line = NSBezierPath()
        for (index, value) in frames.enumerated() {
            let point = NSPoint(x: chart.minX + CGFloat(index) / 119 * chart.width,
                               y: chart.maxY - CGFloat(min(1, max(0, value / ceiling))) * chart.height)
            if index == 0 { line.move(to: point) } else { line.line(to: point) }
        }
        NSColor.controlAccentColor.setStroke(); line.lineWidth = 1; line.stroke()
        let guide = NSBezierPath()
        let y = chart.maxY - CGFloat(budget / ceiling) * chart.height
        guide.move(to: NSPoint(x: chart.minX, y: y)); guide.line(to: NSPoint(x: chart.maxX, y: y))
        NSColor.secondaryLabelColor.withAlphaComponent(0.5).setStroke(); guide.setLineDash([2, 2], count: 2, phase: 0); guide.stroke()
    }
}
