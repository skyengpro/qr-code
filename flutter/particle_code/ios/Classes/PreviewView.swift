import AVFoundation
import Flutter
import UIKit

/// Platform view `particle_code/preview`: the camera preview of the scanner's session.
final class PreviewFactory: NSObject, FlutterPlatformViewFactory {
    private let controller: ScannerController

    init(controller: ScannerController) {
        self.controller = controller
        super.init()
    }

    func create(
        withFrame frame: CGRect, viewIdentifier viewId: Int64, arguments args: Any?
    ) -> FlutterPlatformView {
        PreviewPlatformView(frame: frame, controller: controller)
    }

    /// The view takes no creation params (Dart passes null).
    func createArgsCodec() -> FlutterMessageCodec & NSObjectProtocol {
        FlutterStandardMessageCodec.sharedInstance()
    }
}

private final class PreviewPlatformView: NSObject, FlutterPlatformView {
    private let preview: PreviewView

    init(frame: CGRect, controller: ScannerController) {
        preview = PreviewView(frame: frame, controller: controller)
        super.init()
    }

    func view() -> UIView {
        preview
    }
}

/// A view whose backing layer is an `AVCaptureVideoPreviewLayer` on the controller's session.
///
/// The session object exists before `start` (it just has no input yet), so binding the layer
/// at creation works whether the view appears before or after the camera starts: the preview
/// fills in as soon as the session runs.
final class PreviewView: UIView {
    override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

    /// `layerClass` guarantees the type. Anchors are converted through this layer.
    var previewLayer: AVCaptureVideoPreviewLayer {
        layer as! AVCaptureVideoPreviewLayer  // swiftlint:disable:this force_cast
    }

    private var startObserver: NSObjectProtocol?

    init(frame: CGRect, controller: ScannerController) {
        super.init(frame: frame)
        backgroundColor = .black
        clipsToBounds = true
        previewLayer.session = controller.session
        previewLayer.videoGravity = .resizeAspectFill
        // The preview connection only exists once the session has an input, which is after
        // this view is laid out when it was created before `start`.
        startObserver = NotificationCenter.default.addObserver(
            forName: .AVCaptureSessionDidStartRunning, object: controller.session, queue: .main
        ) { [weak self] _ in
            self?.updateRotation()
        }
        controller.register(preview: self)
    }

    required init?(coder: NSCoder) {
        fatalError("PreviewView is created in code only")
    }

    deinit {
        if let observer = startObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // The layer is the view's own layer and follows its bounds; what is left is turning
        // the picture upright for the current interface orientation.
        updateRotation()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        updateRotation()
    }

    /// Aligns the preview connection with the interface orientation. Only the preview is
    /// rotated, never the data output (see `ScannerController.normalizedAnchors`).
    private func updateRotation() {
        guard let connection = previewLayer.connection,
            let orientation = window?.windowScene?.interfaceOrientation
        else { return }
        if #available(iOS 17.0, *) {
            let angle: CGFloat
            switch orientation {
            case .portrait: angle = 90
            case .portraitUpsideDown: angle = 270
            case .landscapeLeft: angle = 180
            default: angle = 0  // .landscapeRight is the sensor's native orientation
            }
            if connection.isVideoRotationAngleSupported(angle) {
                connection.videoRotationAngle = angle
            }
        } else if connection.isVideoOrientationSupported {
            switch orientation {
            case .portraitUpsideDown: connection.videoOrientation = .portraitUpsideDown
            case .landscapeLeft: connection.videoOrientation = .landscapeLeft
            case .landscapeRight: connection.videoOrientation = .landscapeRight
            default: connection.videoOrientation = .portrait
            }
        }
    }
}
