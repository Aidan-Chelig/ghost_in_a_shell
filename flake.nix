{
  description = "ARG horror Linux guest (RISC-V, Nix, QEMU/crosvm)";


  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = inputs@{ self, nixpkgs, ... }:
    let
      lib = nixpkgs.lib;

      supportedHostSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = f:
        lib.genAttrs supportedHostSystems (system: f system);

mkGuest = hostSystem:
  let
    hostPkgs = import nixpkgs {
      system = hostSystem;
    };
  in
  lib.nixosSystem {
    system = hostSystem;
    specialArgs = {
      inherit hostPkgs;
    };
    modules = [
      ./guest/base.nix
      ({ ... }: {
        nixpkgs.buildPlatform = hostSystem;
        nixpkgs.hostPlatform = "riscv64-linux";
        nixpkgs.config.allowUnsupportedSystem = true;
      })
    ];
  };
    in
    {
      nixosConfigurations =
        lib.genAttrs supportedHostSystems (hostSystem: mkGuest hostSystem);

      packages = forAllSystems (hostSystem:
        let
          guest = self.nixosConfigurations.${hostSystem};
          pkgs = import nixpkgs {
            system = hostSystem;
          };
        in
        {
          default = guest.config.system.build.argRawImage;

          guest-kernel = guest.config.system.build.kernel;
          guest-initrd = guest.config.system.build.initialRamdisk;
          guest-toplevel = guest.config.system.build.toplevel;
          guest-image = guest.config.system.build.argRawImage;

          run-qemu = pkgs.writeShellApplication {
            name = "run-qemu";
            runtimeInputs = with pkgs; [ qemu e2fsprogs coreutils ];
text = ''
  set -euo pipefail

  KERNEL="${guest.config.system.build.kernel}/Image"
  INITRD="${guest.config.system.build.initialRamdisk}/initrd"
  SRC_DISK="${guest.config.system.build.argRawImage}/argvm-riscv64.img"

  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  DISK="$TMPDIR/argvm-riscv64.img"
  cp --reflink=auto "$SRC_DISK" "$DISK"
  chmod u+w "$DISK"

  exec qemu-system-riscv64 \
    -machine virt \
    -m 1024 \
    -smp 2 \
    -nographic \
    -bios default \
    -kernel "$KERNEL" \
    -initrd "$INITRD" \
    -append "console=ttyS0 root=/dev/vda rw loglevel=7 systemd.log_level=debug" \
    -drive "file=$DISK,format=raw,if=virtio"
'';
          };

          run-crosvm = pkgs.writeShellApplication {
            name = "run-crosvm";
            runtimeInputs = with pkgs; [ crosvm coreutils ];
            text = ''
              set -euo pipefail

              KERNEL="${guest.config.system.build.kernel}/Image"
              INITRD="${guest.config.system.build.initialRamdisk}/initrd"
              DISK="${guest.config.system.build.argRawImage}/${guest.config.image.fileName}"

              exec crosvm run \
                --riscv64 \
                --mem 1024 \
                --cpus 2 \
                --initrd "$INITRD" \
                --block "$DISK,ro=false" \
                --serial "type=stdout,hardware=virtio-console,num=1" \
                --params "console=hvc0 root=/dev/vda rw" \
                "$KERNEL"
            '';
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              pkg-config
              qemu_full
              crosvm
              e2fsprogs
              dosfstools
              mtools
              util-linux
              fd
              ripgrep
              just
              nixpkgs-fmt
            ];
          };
        });
    };
}
