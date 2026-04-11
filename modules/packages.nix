{ inputs, ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

      # Use cleanSource so non-Cargo assets like ./assets are preserved.
      src = pkgs.lib.cleanSource ../.;

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
          xdotool
        ];
      };

      cargoArtifacts = craneLib.buildDepsOnly (
        commonArgs
        // {
          pname = "artifacts";
          version = "0.1.0";
          cargoExtraArgs = "--bin host --bin agent";
        }
      );

      hostUnwrapped = craneLib.buildPackage (
        commonArgs
        // {
          pname = "host-unwrapped";
          version = "0.1.0";
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin host";

          postInstall = ''
            mkdir -p $out/share/host
            cp -r crates/host/assets $out/share/host/assets
          '';
        }
      );

      agent = craneLib.buildPackage (
        commonArgs
        // {
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
        inherit agent;
        default = hostUnwrapped;
      };
    };
}
