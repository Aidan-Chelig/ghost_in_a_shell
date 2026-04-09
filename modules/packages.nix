{ inputs, ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
      src = craneLib.cleanCargoSource ../.;

      commonArgs = {
        inherit src;
        strictDeps = true;

        nativeBuildInputs = with pkgs; [
          pkg-config
          clang
          lld
          makeWrapper
        ];

        buildInputs = with pkgs; [
          alsa-lib
          udev
          vulkan-loader
          libxkbcommon
          wayland
          xkeyboard_config
          libXcursor
          libXrandr
          libXi
          alsa-lib
          xdotool
        ];
      };

      cargoArtifacts = craneLib.buildDepsOnly (
        commonArgs // {
          cargoExtraArgs = "--bin host --bin agent";
        }
      );

      hostUnwrapped = craneLib.buildPackage (
        commonArgs // {
          pname = "host";
          version = "0.1.0";
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin host";

          postFixup = ''
            wrapProgram $out/bin/host \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath [
              pkgs.alsa-lib
              pkgs.udev
              pkgs.vulkan-loader
              pkgs.libxkbcommon
              pkgs.wayland
              pkgs.xdotool
            ]} \
      --set ALSA_PLUGIN_DIR ${pkgs.pipewire}/lib/alsa-lib \
      --set ALSA_CONFIG_PATH ${pkgs.alsa-lib}/share/alsa/alsa.conf \
      --set HOST_ASSET_DIR $out/share/host/assets \
              --set XKB_CONFIG_ROOT ${pkgs.xkeyboard_config}/share/X11/xkb
          '';
        }
      );

      agent = craneLib.buildPackage (
        commonArgs // {
          pname = "agent";
          version = "0.1.0";
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin agent";
        }
      );

    in
    {
      packages = {
        host-unwrapped = hostUnwrapped;
        agent = agent;
        default = hostUnwrapped;
      };
    };
}
