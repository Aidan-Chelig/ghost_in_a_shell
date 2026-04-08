{ lib, ... }:
{
  perSystem =
    { pkgs, system, guests, ... }:
    let
      rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;

      qemuKernelPath =
        guestName:
        let
          guest = guests.${guestName};
        in
        if guestName == "x86_64"
        then "${guest.config.system.build.kernel}/bzImage"
        else "${guest.config.system.build.kernel}/Image";

      crosvmKernelPath =
        guestName:
        let
          guest = guests.${guestName};
        in
        if guestName == "x86_64"
        then "${guest.config.system.build.kernel}/bzImage"
        else throw "crosvm runner is only enabled for x86_64 guest right now";

      consoleFor = guestName: "ttyS0";

      mkRunQemu =
        guestName:
        let
          guest = guests.${guestName};
          kernel = qemuKernelPath guestName;
          console = consoleFor guestName;
          machine =
            if guestName == "x86_64" then "-machine q35,accel=kvm:tcg" else "-machine virt";
          qemuBin = if guestName == "x86_64" then "qemu-system-x86_64" else "qemu-system-riscv64";
          maybeBios = if guestName == "riscv64" then "-bios default" else "";
        in
        pkgs.writeShellApplication {
          name = "run-qemu-${guestName}";
          runtimeInputs = with pkgs; [ qemu_full coreutils findutils ];
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

            exec ${qemuBin} \
              ${machine} \
              -m 1024 \
              -smp 2 \
              -nographic \
              ${maybeBios} \
              -kernel "$KERNEL" \
              -initrd "$INITRD" \
              -append "console=${console} root=/dev/vda rw loglevel=7" \
              -drive "file=$DISK,format=raw,if=virtio"
          '';
        };

      mkRunCrosvm =
        guestName:
        let
          guest = guests.${guestName};
          kernel = crosvmKernelPath guestName;
          console = consoleFor guestName;
        in
        pkgs.writeShellApplication {
          name = "run-crosvm-${guestName}";
          runtimeInputs = with pkgs; [ crosvm coreutils findutils ];
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

      hostGame = pkgs.rustPlatform.buildRustPackage {
        pname = "arg-host";
        version = "0.1.0";
        src = ../.;
        cargoLock = {
          lockFile = ../Cargo.lock;
        };

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
    in
    {
      packages = {
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

        arg-host = hostGame;
      };
    };
}
