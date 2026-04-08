{
  description = "ARG horror Linux guest(s)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { self, nixpkgs, ... }:
    let
      lib = nixpkgs.lib;
      hostSystem = "x86_64-linux";
      pkgsFor = system: import nixpkgs { inherit system; };
      hostPkgs = pkgsFor hostSystem;
      /*
        Generic guest builder.

        guestSystem:
          "x86_64-linux"
          "riscv64-linux"

        backend:
          "qemu"
          "crosvm"
      */
      mkGuest = { guestSystem }:
        lib.nixosSystem {
          system = hostSystem;
          specialArgs = {
            inherit hostPkgs guestSystem;
          };
          modules = [
            ./guest/base.nix
            ({ ... }: {
              nixpkgs.buildPlatform = hostSystem;
              nixpkgs.hostPlatform = guestSystem;
              nixpkgs.config.allowUnsupportedSystem = true;
            })
          ];
        };

      guests = {
        x86_64 = mkGuest { guestSystem = "x86_64-linux"; };
        riscv64 = mkGuest { guestSystem = "riscv64-linux"; };
      };

      # Kernel paths differ by arch/runtime
      qemuKernelPath = guestName:
        let guest = guests.${guestName}; in
        if guestName == "x86_64"
        then "${guest.config.system.build.kernel}/bzImage"
        else "${guest.config.system.build.kernel}/Image";

      crosvmKernelPath = guestName:
        let guest = guests.${guestName}; in
        if guestName == "x86_64"
        then "${guest.config.system.build.kernel}/bzImage"
        else throw "crosvm runner is only enabled for x86_64 guest right now";

      consoleFor = guestName:
        if guestName == "x86_64" then "ttyS0" else "ttyS0";

      mkRunQemu = guestName:
        let
          guest = guests.${guestName};
          kernel = qemuKernelPath guestName;
          console = consoleFor guestName;
          machine =
            if guestName == "x86_64" then "-machine q35,accel=kvm:tcg"
            else "-machine virt";
        in
        hostPkgs.writeShellApplication {
          name = "run-qemu-${guestName}";
          runtimeInputs = with hostPkgs; [ qemu_full coreutils findutils ];
          text = ''
            set -euo pipefail

            KERNEL="${kernel}"
            INITRD="${guest.config.system.build.initialRamdisk}/initrd"
            SRC_DISK="$(find ${guest.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1)"

            TMPDIR="$(mktemp -d)"
            trap 'rm -rf "$TMPDIR"' EXIT

            DISK="$TMPDIR/${guestName}.img"
            cp --reflink=auto "$SRC_DISK" "$DISK"
            chmod u+w "$DISK"

            exec qemu-system-${if guestName == "x86_64" then "x86_64" else "riscv64"} \
              ${machine} \
              -m 1024 \
              -smp 2 \
              -nographic \
              ${if guestName == "riscv64" then "-bios default" else ""} \
              -kernel "$KERNEL" \
              -initrd "$INITRD" \
              -append "console=${console} root=/dev/vda rw loglevel=7" \
              -drive "file=$DISK,format=raw,if=virtio"
          '';
        };

      mkRunCrosvm = guestName:
        let
          guest = guests.${guestName};
          kernel = crosvmKernelPath guestName;
          console = consoleFor guestName;
        in
        hostPkgs.writeShellApplication {
          name = "run-crosvm-${guestName}";
          runtimeInputs = with hostPkgs; [ crosvm coreutils findutils ];
          text = ''
            set -euo pipefail

            KERNEL="${kernel}"
            INITRD="${guest.config.system.build.initialRamdisk}/initrd"
            SRC_DISK="$(find ${guest.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1)"

            TMPDIR="$(mktemp -d)"
            trap 'rm -rf "$TMPDIR"' EXIT

            DISK="$TMPDIR/${guestName}.img"
            cp --reflink=auto "$SRC_DISK" "$DISK"
            chmod u+w "$DISK"

            exec crosvm run \
              --mem size=1024 \
              --cpus num-cores=2 \
              --initrd "$INITRD" \
              --block path="$DISK",root=true,ro=false \
              --serial type=stdout,hardware=serial,num=1,console=true,stdin=true \
              --params "console=${console} loglevel=7" \
              "$KERNEL"
          '';
        };

    in {
      nixosConfigurations = {
        guest-x86_64 = guests.x86_64;
        guest-riscv64 = guests.riscv64;
      };

      packages.${hostSystem} = {
        default = guests.x86_64.config.system.build.argRootFs;

        guest-x86_64-image = guests.x86_64.config.system.build.argRootFs;
        guest-x86_64-kernel = guests.x86_64.config.system.build.kernel;
        guest-x86_64-initrd = guests.x86_64.config.system.build.initialRamdisk;
        run-qemu-x86_64 = mkRunQemu "x86_64";
        run-crosvm-x86_64 = mkRunCrosvm "x86_64";

        guest-riscv64-image = guests.riscv64.config.system.build.argRootFs;
        guest-riscv64-kernel = guests.riscv64.config.system.build.kernel;
        guest-riscv64-initrd = guests.riscv64.config.system.build.initialRamdisk;
        run-qemu-riscv64 = mkRunQemu "riscv64";
      };

      devShells.${hostSystem}.default = hostPkgs.mkShell {
          shellHook = ''export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
            hostPkgs.lib.makeLibraryPath [
              hostPkgs.alsa-lib
              hostPkgs.udev
              hostPkgs.vulkan-loader
              hostPkgs.libxkbcommon
              hostPkgs.wayland
              hostPkgs.xdotool
            ]
          }"'';


        packages = with hostPkgs; [
          cargo rustc rustfmt clippy
          pkg-config
          qemu_full
          crosvm
          e2fsprogs
          util-linux
          fd ripgrep just nixpkgs-fmt
        ];
      };
    };
}
