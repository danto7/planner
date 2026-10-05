{ lib, mkShell, callPackage, cargo, rustc, rustfmt, clippy, rust-analyzer, rustPlatform }:
let
  runtimeLibs = callPackage ./libs.nix { };
in
mkShell {
  packages = [ cargo rustc rustfmt clippy rust-analyzer ];
  buildInputs = runtimeLibs;

  # winit/glutin dlopen these at runtime; see libs.nix.
  LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
  RUST_SRC_PATH = "${rustPlatform.rustLibSrc}";
}
