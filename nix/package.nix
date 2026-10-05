{ lib, rustPlatform, makeWrapper, callPackage }:
let
  cargoToml = lib.importTOML ../Cargo.toml;
  runtimeLibs = callPackage ./libs.nix { };
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ makeWrapper ];
  buildInputs = runtimeLibs;

  postFixup = ''
    wrapProgram $out/bin/planner \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
  '';

  meta = {
    description = cargoToml.package.description;
    mainProgram = "planner";
    platforms = lib.platforms.linux;
  };
}
