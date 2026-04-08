{ rustPlatform, pkg-config, clang, lld }:

rustPlatform.buildRustPackage {
  pname = "ghost-agent";
  version = "0.1.0";
  src = ../../.;
  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [ "--bin" "ghost-agent" ];

  nativeBuildInputs = [
    pkg-config
    clang
    lld
  ];
}
