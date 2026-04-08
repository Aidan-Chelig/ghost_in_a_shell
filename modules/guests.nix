{ inputs, lib, ... }:
let
  hostSystem = "x86_64-linux";

  hostPkgs = import inputs.nixpkgs {
    system = hostSystem;
    overlays = [ (import inputs.rust-overlay) ];
    config.allowUnfree = true;
  };

  guestDefs = {
    x86_64 = {
      guestSystem = "x86_64-linux";
      qemu = {
        kernelPath = guest: "${guest.config.system.build.kernel}/bzImage";
        systemBin = "qemu-system-x86_64";
        machine = "-machine q35,accel=kvm:tcg";
        bios = null;
        console = "ttyS0";
      };
      crosvm = {
        enabled = true;
        kernelPath = guest: "${guest.config.system.build.kernel}/bzImage";
        console = "ttyS0";
      };
    };

    riscv64 = {
      guestSystem = "riscv64-linux";
      qemu = {
        kernelPath = guest: "${guest.config.system.build.kernel}/Image";
        systemBin = "qemu-system-riscv64";
        machine = "-machine virt";
        bios = "default";
        console = "ttyS0";
      };
      crosvm = {
        enabled = false;
        kernelPath = _: throw "crosvm is not enabled for riscv64 on this host";
        console = "ttyS0";
      };
    };
  };

  mkGuest =
    name:
    let
      def = guestDefs.${name};
    in
    inputs.nixpkgs.lib.nixosSystem {
      system = hostSystem;
      specialArgs = {
        inherit hostPkgs;
        guestSystem = def.guestSystem;
      };
      modules = [
        ../guest/base.nix
        ({ ... }: {
          nixpkgs.buildPlatform = hostSystem;
          nixpkgs.hostPlatform = def.guestSystem;
          nixpkgs.config.allowUnsupportedSystem = true;
        })
      ];
    };

  guests = lib.mapAttrs (name: _: mkGuest name) guestDefs;
in
{
  flake.nixosConfigurations = {
    guest-x86_64 = guests.x86_64;
    guest-riscv64 = guests.riscv64;
  };

  perSystem = { ... }: {
    _module.args = {
      inherit guestDefs guests hostPkgs;
    };
  };
}
