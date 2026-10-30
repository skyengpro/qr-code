import UIKit

/// `particle_code/display`: full brightness and no auto-lock while a code is on screen.
///
/// iOS keeps a brightness set by an app until the user changes it, even after the app goes
/// to the background, so the previous value is restored whenever the app resigns active
/// (home gesture, control center, a call) and applied again when it becomes active, as long
/// as the Dart side still says a code is showing. All methods run on the main thread.
final class DisplayController {
    private var showing = false
    private var savedBrightness: CGFloat?
    private var savedIdleTimerDisabled = false
    private var observers: [NSObjectProtocol] = []

    init() {
        let center = NotificationCenter.default
        observers.append(
            center.addObserver(
                forName: UIApplication.willResignActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                self?.restore()
            })
        observers.append(
            center.addObserver(
                forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                guard let self = self, self.showing else { return }
                self.apply()
            })
    }

    deinit {
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    /// Idempotent: `true` twice remembers the original brightness only once.
    func setShowing(_ on: Bool) {
        if on {
            showing = true
            apply()
        } else {
            showing = false
            restore()
        }
    }

    /// Forces maximum brightness and keeps the screen on, remembering what was there.
    private func apply() {
        if savedBrightness == nil {
            savedBrightness = UIScreen.main.brightness
            savedIdleTimerDisabled = UIApplication.shared.isIdleTimerDisabled
        }
        UIScreen.main.brightness = 1
        UIApplication.shared.isIdleTimerDisabled = true
    }

    /// Puts back what `apply` replaced. Keeps `showing`, so becoming active re-applies.
    private func restore() {
        guard let brightness = savedBrightness else { return }
        UIScreen.main.brightness = brightness
        UIApplication.shared.isIdleTimerDisabled = savedIdleTimerDisabled
        savedBrightness = nil
    }
}
