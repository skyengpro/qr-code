// Native Assets build hook: Flutter runs it for every app that depends on this plugin and
// bundles the result. It compiles crates/particlecode-ffi (the flutter_rust_bridge library)
// with cargo for the target being built (Android ABIs, iOS device and simulator, the host
// for `flutter test`), using the toolchain and targets of
// crates/particlecode-ffi/rust-toolchain.toml.
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    await const FlutterRustBridgeNativeAssetsBuilder(
      cratePath: '../../crates/particlecode-ffi',
    ).run(input: input, output: output);
  });
}
