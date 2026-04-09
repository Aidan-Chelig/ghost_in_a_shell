{ ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
    in
    {
      devShells.default = pkgs.mkShell {
  shellHook = ''
    export ALSA_PLUGIN_DIR="${pkgs.pipewire}/lib/alsa-lib"
    export ALSA_CONFIG_PATH="${pkgs.alsa-lib}/share/alsa/alsa.conf"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
      pkgs.alsa-lib
      pkgs.pipewire
      pkgs.pipewire.jack
              pkgs.udev
              pkgs.vulkan-loader
              pkgs.libxkbcommon
              pkgs.wayland
              pkgs.xdotool
    ]}"
  '';

 packages = with pkgs; [
    cargo
    rustc
    pkg-config
    clang
    lld
    alsa-lib
    pipewire
    pipewire.jack
    udev
          vulkan-loader
          libxkbcommon
          wayland
          xdotool
  ];
      };
    };
}
