{ rustPlatform, pkg-config, clang, lld, alsa-lib, udev, vulkan-loader, libxkbcommon, wayland, xorg }:

rustPlatform.buildRustPackage {
  pname = "agent";
  version = "0.1.0";
  src = ../../.;
  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [ "--bin" "agent" ];

  nativeBuildInputs = [
    pkg-config
    clang
    lld
  ];

  buildInputs = [
    alsa-lib
    udev
    vulkan-loader
    libxkbcommon
    wayland
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
  ];
}
