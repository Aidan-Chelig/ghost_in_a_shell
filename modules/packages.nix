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
        ];

        buildInputs = with pkgs; [
          alsa-lib
          udev
          vulkan-loader
          libxkbcommon
          wayland
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi
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

      story-agent = craneLib.buildPackage (
        commonArgs // {
          pname = "story-agent";
          version = "0.1.0";
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin story-agent";
        }
      );
    in
    {
      packages = {
        host-unwrapped = hostUnwrapped;
        agent = agent;
        story-agent = story-agent;

        # optional during transition
        default = hostUnwrapped;
      };
    };
}
