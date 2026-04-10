{ ... }:
{
  perSystem =
    { pkgs, system, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;

      isLinux = pkgs.stdenv.hostPlatform.isLinux;
      isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
    in
    {
      devShells.default = pkgs.mkShell {
        packages =
          [
            rustToolchain
            pkgs.pkg-config
            pkgs.rust-analyzer
          ]
          ++ pkgs.lib.optionals isLinux [
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
          ]
          ++ pkgs.lib.optionals isDarwin [
            #pkgs.darwin.apple_sdk.frameworks.AppKit
            #pkgs.darwin.apple_sdk.frameworks.CoreAudio
            #pkgs.darwin.apple_sdk.frameworks.AudioToolbox
            #pkgs.darwin.apple_sdk.frameworks.AudioUnit
            #pkgs.darwin.apple_sdk.frameworks.CoreFoundation
            #pkgs.darwin.apple_sdk.frameworks.CoreGraphics
            #pkgs.darwin.apple_sdk.frameworks.Foundation
            #pkgs.darwin.apple_sdk.frameworks.IOKit
            #pkgs.darwin.apple_sdk.frameworks.Metal
            #pkgs.darwin.apple_sdk.frameworks.QuartzCore
          ];

        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

        ALSA_PLUGIN_DIR = pkgs.lib.optionalString isLinux "${pkgs.pipewire}/lib/alsa-lib";

        LD_LIBRARY_PATH = pkgs.lib.optionalString isLinux (
          pkgs.lib.makeLibraryPath [
            pkgs.vulkan-loader
            pkgs.libx11
            pkgs.alsa-lib
            pkgs.libxi
            pkgs.pipewire
            pkgs.libxcursor
            pkgs.libxkbcommon
            pkgs.wayland
          ]
        );
      };
    };
}
