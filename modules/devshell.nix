{ ... }:
{
  perSystem =
    { pkgs, system, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
    in
    {
      devShells.default = pkgs.mkShell {
        buildInputs =
          [
            rustToolchain
            pkgs.pkg-config
          ]
          ++ pkgs.lib.optionals (pkgs.lib.strings.hasInfix "linux" system) [
            pkgs.alsa-lib
            pkgs.bashInteractive
            pkgs.pipewire
            pkgs.vulkan-loader
            pkgs.vulkan-tools
            pkgs.libudev-zero
            pkgs.libx11
            pkgs.libxcursor
            pkgs.libxi
            pkgs.libxrandr
            pkgs.libxkbcommon
            pkgs.wayland
            pkgs.rust-analyzer
          ];

        ALSA_PLUGIN_DIR = "${pkgs.pipewire}/lib/alsa-lib";
        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
          pkgs.vulkan-loader
          pkgs.libx11
          pkgs.alsa-lib
          pkgs.libxi
          pkgs.pipewire
          pkgs.libxcursor
          pkgs.libxkbcommon
          pkgs.wayland
        ];
      };
    };
}
