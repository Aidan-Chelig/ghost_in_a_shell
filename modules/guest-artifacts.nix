{ inputs, lib, withSystem, ... }:
let
  hostSystem = "x86_64-linux";
in
{
  perSystem =
    { pkgs, config, ... }:
    let
      guestDefs = config._module.args.guestDefs or (throw "guestDefs missing");

      guestX86 = inputs.self.nixosConfigurations.guest-x86_64;
      guestRiscv = inputs.self.nixosConfigurations.guest-riscv64;

      guestImagePath = guest:
        ''$(find ${guest.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1)'';

      mkRunQemu =
        guestName: guest: def:
        let
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
        guestName: guest: def:
        pkgs.writeShellApplication {
          name = "run-crosvm-${guestName}";
          runtimeInputs = with pkgs; [ crosvm coreutils findutils ];
          text = ''
            set -euo pipefail

            KERNEL="${def.crosvm.kernelPath guest}"
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
              --params "console=${def.crosvm.console} loglevel=7" \
              --vsock cid=3 \
              "$KERNEL"
          '';
        };

      host = pkgs.symlinkJoin {
        name = "host";
        paths = [ config.packages.host-unwrapped ];
        nativeBuildInputs = [ pkgs.makeWrapper ];

        postBuild = ''
          wrapProgram $out/bin/host \
            --set ARGVM_CROSVM ${pkgs.lib.getExe pkgs.crosvm} \
            --set ARGVM_KERNEL ${guestDefs.x86_64.crosvm.kernelPath guestX86} \
            --set ARGVM_INITRD ${guestX86.config.system.build.initialRamdisk}/initrd \
            --set ARGVM_ROOTFS $(find ${guestX86.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1) \
            --set ARGVM_CONSOLE ${guestDefs.x86_64.crosvm.console}
        '';
      };
    in
    {
      packages = {
        guest-x86_64-image = guestX86.config.system.build.argRootFs;
        guest-x86_64-kernel = guestX86.config.system.build.kernel;
        guest-x86_64-initrd = guestX86.config.system.build.initialRamdisk;
        run-qemu-x86_64 = mkRunQemu "x86_64" guestX86 guestDefs.x86_64;
        run-crosvm-x86_64 = mkRunCrosvm "x86_64" guestX86 guestDefs.x86_64;

        guest-riscv64-image = guestRiscv.config.system.build.argRootFs;
        guest-riscv64-kernel = guestRiscv.config.system.build.kernel;
        guest-riscv64-initrd = guestRiscv.config.system.build.initialRamdisk;
        run-qemu-riscv64 = mkRunQemu "riscv64" guestRiscv guestDefs.riscv64;

        host = host;
        default = host;
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
