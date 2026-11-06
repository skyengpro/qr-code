/// Particle Code for Flutter: show payloads as an animated field of colored dots, and
/// scan them with the camera.
///
/// **Sender.** Create a [ParticleEncoder] from a payload (typically COSE/CBOR) and show it
/// with a [ParticleCodeView]. Call `await ParticleCode.init()` once at start-up: the
/// encoder talks to the Rust library through flutter_rust_bridge; painting is pure Dart
/// ([ParticleCodePainter]).
///
/// **Scanner.** A [ParticleScanner] controls the native camera pipeline and streams
/// [ScanEvent]s; [ParticleScannerPreview] shows the camera.
library;

export 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;

export 'src/display.dart' show ParticleCodeDisplay;
export 'src/encoder.dart'
    show
        DisplayInfo,
        EncoderInfo,
        EncoderOptions,
        ParticleEncoder,
        Shape,
        ShapeMode,
        livenessVerify;
export 'src/exception.dart' show ParticleCodeException;
export 'src/init.dart' show ParticleCode;
export 'src/frame_spec.dart'
    show
        AnchorSpec,
        CodeDims,
        CodeProfile,
        FrameDot,
        FrameDots,
        FrameHeader,
        FrameSpec,
        ParticleBand,
        ParticleSpec,
        Swatch,
        SwatchColor,
        SwatchKind;
export 'src/motion.dart'
    show
        DotMotion,
        MotionConfig,
        Mood,
        anchorScale,
        breathPeriod,
        crossfadeAlpha,
        crossfadeDuration,
        dataFrameInterval,
        dotOffset,
        dotRadiusScale,
        maxDrift,
        particleGlide,
        particlePose,
        radiusPulse,
        revealDuration,
        revealOpacity,
        rippleDuration,
        rippleLighten,
        seedFromSession,
        trailLength;
export 'src/painter.dart' show ParticleCodePainter, defaultMargin;
export 'src/palette.dart' show ParticleCodePalette;
export 'src/scanner/errors.dart'
    show ParticleScannerError, ParticleScannerException;
export 'src/scanner/events.dart'
    show
        ScanDone,
        ScanError,
        ScanEvent,
        ScanFrame,
        ScanHint,
        ScanLiveness,
        ScanSegment,
        ScanStatus;
export 'src/scanner/preview.dart'
    show
        ParticleScannerPreview,
        ScanAnchorsPainter,
        ScanOverlay,
        ScanOverlayBuilder;
export 'src/scanner/progress_ring.dart'
    show ScanProgressRing, ScanProgressRingPainter;
export 'src/scanner/scanner.dart' show ParticleScanner;
export 'src/view.dart' show ParticleCodeView;
