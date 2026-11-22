import QuartzCore
#if os(iOS)
import UIKit
@MainActor private var jadixReduceMotion: Bool { UIAccessibility.isReduceMotionEnabled }
#else
import AppKit
@MainActor private var jadixReduceMotion: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
#endif

extension JadixViewAppearance {
    func animate(from: JadixVisualStyle? = nil, to end: JadixVisualStyle, duration: TimeInterval,
                 completion: (() -> Void)? = nil) {
        cancel()
        let start = from ?? current
        guard duration > 0, view?.window != nil, !jadixReduceMotion else {
            render(end); completion?(); return
        }
        render(start)
        let began = CACurrentMediaTime()
        let next = Timer(timeInterval: 1 / 120, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                let t = min(1, CGFloat((CACurrentMediaTime() - began) / duration))
                self.render(start.interpolated(to: end, at: t * t * (3 - 2 * t)))
                if t >= 1 { self.cancel(); completion?() }
            }
        }
        timer = next; RunLoop.main.add(next, forMode: .common)
    }
    func update(_ wire: [String: Any], background: JadixColor) {
        let snapshot = NSDictionary(dictionary: wire), now = CACurrentMediaTime()
        let changed = motion.wire?.isEqual(snapshot) != true || motion.background?.isEqual(background) != true
        if changed {
            // Sample at the interruption timestamp before replacing the target.
            if motion.target != nil { tickMotion(now) }
            let previous = motion.base, target = JadixVisualStyle(wire, background: background)
            motion.target = target; motion.wire = snapshot; motion.background = background
            motion.from = previous; motion.elapsed = 0
            motion.transition = previous == nil ? nil : wire["transition"] as? [String: Any]
            let animation = wire["animation"] as? [String: Any]
            let animationWire = animation.map { NSDictionary(dictionary: $0) }
            let sameAnimation = animationWire.map { motion.animationWire?.isEqual($0) == true } ?? (motion.animationWire == nil)
            if !sameAnimation {
                motion.animation = animation; motion.animationWire = animationWire; motion.animationElapsed = 0
            }
        }
        if motion.last == 0 { motion.last = now }
        tickMotion(now, layout: true)
    }
    private func tickMotion(_ now: Double, layout: Bool = false) {
        guard let target = motion.target, let view else { timer?.invalidate(); timer = nil; return }
        let visible = jadixSurfaceVisible(view)
        let dt = visible ? max(0, now - motion.last) : 0
        motion.last = now
        if jadixReduceMotion {
            motion.transition = nil; motion.base = target
            render(target); timer?.invalidate(); timer = nil; return
        }
        if !visible && !layout { scheduleMotion(0.25); return }
        motion.elapsed += dt; motion.animationElapsed += dt
        var base = target
        if let config = motion.transition, let from = motion.from {
            let after = motion.elapsed - (config["delay"] as? Double ?? 0) / 1000
            let duration = (config["duration"] as? Double ?? 0) / 1000
            var progress: CGFloat = 0, done = false
            if after >= 0 {
                if let spring = config["spring"] as? [String: Any] { (progress, done) = JadixVisualMotion.spring(spring, after) }
                else { done = duration <= 0 || after >= duration
                    progress = done ? 1 : JadixVisualMotion.ease(config["easing"] as? [Double] ?? [0.25,0.1,0.25,1], after / duration) }
            }
            if done { motion.transition = nil }
            else { base = JadixVisualMotion.transition(from, target, progress, config["properties"] as? Int ?? 31) }
        }
        motion.base = base
        if let animation = motion.animation {
            let (animated, playing) = JadixVisualMotion.keyframes(animation, motion.animationElapsed, base, view.bounds.height)
            render(animated)
            if !playing { motion.animation = nil }
        } else { render(base) }
        if motion.transition == nil && motion.animation == nil { timer?.invalidate(); timer = nil; return }
        let hz = motion.transition != nil ? 120 : min(120, motion.animation?["frame_rate"] as? Double ?? 120)
        scheduleMotion(visible ? 1 / hz : 0.25)
    }
    private func scheduleMotion(_ interval: Double) {
        if timer != nil && motion.interval == interval { return }
        timer?.invalidate(); motion.interval = interval
        let next = Timer(timeInterval: interval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tickMotion(CACurrentMediaTime()) }
        }
        timer = next; RunLoop.main.add(next, forMode: .common)
    }
}
