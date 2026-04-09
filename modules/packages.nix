{ lib, inputs, ... }:
{
  perSystem =
    { pkgs, guestDefs, guests, hostPkgs, ... }:
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

      guestImagePath = guest:
        ''$(find ${guest.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1)'';

      mkRunQemu =
        guestName:
        let
          def = guestDefs.${guestName};
          guest = guests.${guestName};
          kernel = def.qemu.kernelPath guest;
          console = def.qemu.console;
          qemuBin = def.qemu.systemBin;
          machine = def.qemu.machine;
          biosArg = lib.optionalString (def.qemu.bios != null) ''-bios "${def.qemu.bios}"'';
        in
          pkgs.writeShellApplication {
            name = "run-qemu-${guestName}";
            runtimeInputs = with pkgs; [ qemu_full coreutils findutils ];
            text = ''
            set -euo pipefail

            KERNEL="${kernel}"
            INITRD="${guest.config.system.build.initialRamdisk}/initrd"
            SRC_DISK=${guestImagePath guest}

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
            ${biosArg} \
            -kernel "$KERNEL" \
            -initrd "$INITRD" \
            -append "console=${console} root=/dev/vda rw loglevel=7" \
            -drive "file=$DISK,format=raw,if=virtio"
            '';
            };

            mkRunCrosvm =
            guestName:
            let
            def = guestDefs.${guestName};
            guest = guests.${guestName};
            kernel = def.crosvm.kernelPath guest;
            console = def.crosvm.console;
            in
            pkgs.writeShellApplication {
            name = "run-crosvm-${guestName}";
            runtimeInputs = with pkgs; [ crosvm coreutils findutils ];
            text = ''
            set -euo pipefail

            KERNEL="${kernel}"
            INITRD="${guest.config.system.build.initialRamdisk}/initrd"
            SRC_DISK=${guestImagePath guest}

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
            --vsock cid=3 \
            "$KERNEL"
            '';
          };


      cargoArtifacts = craneLib.buildDepsOnly (
        commonArgs
        // {
          cargoExtraArgs = "--bin host";
        }
      );

      hostUnwrapped = craneLib.buildPackage (
        commonArgs
        // {
          pname = "host";
          version = "0.1.0";
          inherit cargoArtifacts;
          cargoExtraArgs = "--bin host";
        }
      );



      guestX86 = guests.x86_64;
      guestX86Def = guestDefs.x86_64;

      host = pkgs.symlinkJoin {
        name = "host";
        paths = [ hostUnwrapped ];
        nativeBuildInputs = [ pkgs.makeWrapper ];

        postBuild = ''
          wrapProgram $out/bin/host \
          --set ARGVM_CROSVM ${pkgs.lib.getExe pkgs.crosvm} \
          --set ARGVM_KERNEL ${guestX86Def.crosvm.kernelPath guestX86} \
          --set ARGVM_INITRD ${guestX86.config.system.build.initialRamdisk}/initrd \
          --set ARGVM_ROOTFS $(find ${guestX86.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1) \
          --set ARGVM_CONSOLE ${guestX86Def.crosvm.console}
          '';
      };
    in
      {
      packages = {
        default = host;

        guest-x86_64-image = guests.x86_64.config.system.build.argRootFs;
        guest-x86_64-kernel = guests.x86_64.config.system.build.kernel;
        guest-x86_64-initrd = guests.x86_64.config.system.build.initialRamdisk;
        run-qemu-x86_64 = mkRunQemu "x86_64";
        run-crosvm-x86_64 = mkRunCrosvm "x86_64";

        guest-riscv64-image = guests.riscv64.config.system.build.argRootFs;
        guest-riscv64-kernel = guests.riscv64.config.system.build.kernel;
        guest-riscv64-initrd = guests.riscv64.config.system.build.initialRamdisk;
        run-qemu-riscv64 = mkRunQemu "riscv64";

        host-unwrapped = hostUnwrapped;
        host = host;
      };

      apps.default = {
        type = "app";
        program = "${host}/bin/host";
      };

      apps.host = {
        type = "app";
        program = "${host}/bin/host";
      };
    };
}
