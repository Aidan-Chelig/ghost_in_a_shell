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
          export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
            pkgs.lib.makeLibraryPath [
              pkgs.alsa-lib
              pkgs.udev
              pkgs.vulkan-loader
              pkgs.libxkbcommon
              pkgs.wayland
              pkgs.xdotool
            ]
          }"
        '';

        packages = with pkgs; [
          rustToolchain
          python3
          rust-analyzer
          rustfmt
          cargo-edit
          cargo-watch
          pkg-config

          alsa-lib
          jack2

          lld
          clang
          just
          bacon
          bugstalker

          udev
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi

          vulkan-tools
          vulkan-headers
          vulkan-loader
          vulkan-validation-layers

          libjack2
          openssl
          gdk-pixbuf
          atk
          pango
          glib
          gtk3
          libsoup_3
          webkitgtk_4_1
          gtk4
          xdotool

          qemu_full
          crosvm
          e2fsprogs
          util-linux
          fd
          ripgrep
          nixpkgs-fmt
        ];
      };
    };
}
