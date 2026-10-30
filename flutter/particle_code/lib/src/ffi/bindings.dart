import 'dart:ffi';

import 'package:ffi/ffi.dart';

import '../exception.dart';
import 'library_loader.dart';

/// The ABI version this package was written against (`PC_ABI_VERSION` in
/// `particlecode.h`).
const int expectedAbiVersion = 1;

/// Opaque `PcEncoder` handle.
final class PcEncoder extends Opaque {}

// Native signatures, matching crates/particlecode-ffi/include/particlecode.h.
typedef _AbiVersionC = Uint32 Function();
typedef _AbiVersionD = int Function();

typedef _StringFreeC = Void Function(Pointer<Utf8>);
typedef _StringFreeD = void Function(Pointer<Utf8>);

typedef _BytesFreeC = Void Function(Pointer<Uint8>, IntPtr);
typedef _BytesFreeD = void Function(Pointer<Uint8>, int);

typedef _LivenessVerifyC =
    Int32 Function(Pointer<Uint8>, IntPtr, Uint16, Uint64, Uint64);
typedef _LivenessVerifyD = int Function(Pointer<Uint8>, int, int, int, int);

typedef _EncoderNewC =
    Pointer<PcEncoder> Function(
      Pointer<Uint8>,
      IntPtr,
      Pointer<Uint8>,
      IntPtr,
      Pointer<Pointer<Utf8>>,
    );
typedef _EncoderNewD =
    Pointer<PcEncoder> Function(
      Pointer<Uint8>,
      int,
      Pointer<Uint8>,
      int,
      Pointer<Pointer<Utf8>>,
    );

typedef _EncoderFreeC = Void Function(Pointer<PcEncoder>);
typedef _EncoderFreeD = void Function(Pointer<PcEncoder>);

typedef _FrameJsonC = Pointer<Utf8> Function(Pointer<PcEncoder>, Uint32);
typedef _FrameJsonD = Pointer<Utf8> Function(Pointer<PcEncoder>, int);

typedef _FrameJsonAtC =
    Pointer<Utf8> Function(Pointer<PcEncoder>, Uint32, Uint64);
typedef _FrameJsonAtD = Pointer<Utf8> Function(Pointer<PcEncoder>, int, int);

typedef _InfoJsonC = Pointer<Utf8> Function(Pointer<PcEncoder>);
typedef _InfoJsonD = Pointer<Utf8> Function(Pointer<PcEncoder>);

typedef _FramesPerPassC = Uint32 Function(Pointer<PcEncoder>);
typedef _FramesPerPassD = int Function(Pointer<PcEncoder>);

typedef _EncoderKeyC = Int32 Function(Pointer<PcEncoder>, Pointer<Uint8>);
typedef _EncoderKeyD = int Function(Pointer<PcEncoder>, Pointer<Uint8>);

/// Hand-written bindings for the encoder part of the Particle Code C ABI.
///
/// Use [ParticleBindings.instance] for the process-wide library (opened lazily by
/// [openParticleLibrary]), or [ParticleBindings.fromLibrary] to bind a specific
/// [DynamicLibrary].
class ParticleBindings {
  /// Binds the `pc_*` symbols of [library] and checks `pc_abi_version`.
  ///
  /// Throws [ParticleCodeException] if a symbol is missing or the library reports an
  /// ABI version other than [expectedAbiVersion].
  factory ParticleBindings.fromLibrary(DynamicLibrary library) {
    final ParticleBindings bindings;
    try {
      bindings = ParticleBindings._(library);
    } on ArgumentError catch (e) {
      // `DynamicLibrary.lookup` throws ArgumentError for a missing symbol.
      throw ParticleCodeException(
        'particlecode native library is missing a symbol: ${e.message}',
      );
    }
    final actual = bindings.abiVersion();
    if (actual != expectedAbiVersion) {
      throw ParticleCodeException(
        'particlecode native library has ABI version $actual, '
        'this package expects $expectedAbiVersion',
      );
    }
    return bindings;
  }

  ParticleBindings._(this.library)
    : abiVersion = library.lookupFunction<_AbiVersionC, _AbiVersionD>(
        'pc_abi_version',
      ),
      stringFree = library.lookupFunction<_StringFreeC, _StringFreeD>(
        'pc_string_free',
      ),
      bytesFree = library.lookupFunction<_BytesFreeC, _BytesFreeD>(
        'pc_bytes_free',
      ),
      livenessVerifyRaw = library
          .lookupFunction<_LivenessVerifyC, _LivenessVerifyD>(
            'pc_liveness_verify',
          ),
      encoderNew = library.lookupFunction<_EncoderNewC, _EncoderNewD>(
        'pc_encoder_new',
      ),
      encoderFree = library.lookupFunction<_EncoderFreeC, _EncoderFreeD>(
        'pc_encoder_free',
      ),
      encoderFreeAddress = library.lookup<NativeFinalizerFunction>(
        'pc_encoder_free',
      ),
      encoderFrameJson = library.lookupFunction<_FrameJsonC, _FrameJsonD>(
        'pc_encoder_frame_json',
      ),
      encoderFrameJsonAt = library.lookupFunction<_FrameJsonAtC, _FrameJsonAtD>(
        'pc_encoder_frame_json_at',
      ),
      encoderInfoJson = library.lookupFunction<_InfoJsonC, _InfoJsonD>(
        'pc_encoder_info_json',
      ),
      encoderFramesPerPass = library
          .lookupFunction<_FramesPerPassC, _FramesPerPassD>(
            'pc_encoder_frames_per_pass',
          ),
      encoderKeyRaw = library.lookupFunction<_EncoderKeyC, _EncoderKeyD>(
        'pc_encoder_key',
      );

  static ParticleBindings? _instance;

  /// The process-wide bindings, opening the native library on first use.
  static ParticleBindings get instance =>
      _instance ??= ParticleBindings.fromLibrary(openParticleLibrary());

  /// Replaces the process-wide bindings; `null` reloads on next use. For tests.
  static set instance(ParticleBindings? value) => _instance = value;

  /// The bound library.
  final DynamicLibrary library;

  /// `uint32_t pc_abi_version(void)`.
  final int Function() abiVersion;

  /// `void pc_string_free(char *s)`.
  final void Function(Pointer<Utf8>) stringFree;

  /// `void pc_bytes_free(uint8_t *ptr, size_t len)`.
  final void Function(Pointer<Uint8>, int) bytesFree;

  /// `int32_t pc_liveness_verify(key, key_len, rolling, now_unix, skew)`.
  final int Function(Pointer<Uint8>, int, int, int, int) livenessVerifyRaw;

  /// `PcEncoder *pc_encoder_new(payload, len, config_json, config_len, out_err)`.
  final Pointer<PcEncoder> Function(
    Pointer<Uint8>,
    int,
    Pointer<Uint8>,
    int,
    Pointer<Pointer<Utf8>>,
  )
  encoderNew;

  /// `void pc_encoder_free(PcEncoder *enc)`.
  final void Function(Pointer<PcEncoder>) encoderFree;

  /// Address of `pc_encoder_free`, for a `NativeFinalizer`.
  final Pointer<NativeFinalizerFunction> encoderFreeAddress;

  /// `char *pc_encoder_frame_json(enc, i)`.
  final Pointer<Utf8> Function(Pointer<PcEncoder>, int) encoderFrameJson;

  /// `char *pc_encoder_frame_json_at(enc, i, unix_seconds)`.
  final Pointer<Utf8> Function(Pointer<PcEncoder>, int, int) encoderFrameJsonAt;

  /// `char *pc_encoder_info_json(enc)`.
  final Pointer<Utf8> Function(Pointer<PcEncoder>) encoderInfoJson;

  /// `uint32_t pc_encoder_frames_per_pass(enc)`.
  final int Function(Pointer<PcEncoder>) encoderFramesPerPass;

  /// `int32_t pc_encoder_key(enc, out)`; `out` has room for 32 bytes.
  final int Function(Pointer<PcEncoder>, Pointer<Uint8>) encoderKeyRaw;

  /// Reads a library-owned string, then releases it with `pc_string_free`.
  ///
  /// Returns `null` for a `NULL` pointer.
  String? takeString(Pointer<Utf8> ptr) {
    if (ptr == nullptr) return null;
    try {
      return ptr.toDartString();
    } finally {
      stringFree(ptr);
    }
  }
}
