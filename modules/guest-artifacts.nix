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

            DISK="$HOME/.local/share/ghost_in_a_shell/rootfs.img"
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
            KERNEL="${def.crosvm.kernelPath guest}"
            INITRD="${guest.config.system.build.initialRamdisk}/initrd"

            DISK="$HOME/.local/share/ghost_in_a_shell/rootfs.img"

            exec crosvm run \
              --disable-sandbox \
              --mem size=1024 \
              --cpus num-cores=2 \
              --initrd "$INITRD" \
              --block path="$DISK",root=true,ro=false \
              --serial type=stdout,hardware=serial,num=1,console=true,stdin=true \
              --params "console=${def.crosvm.console} loglevel=7" \
              "$KERNEL"
          '';
        };

      host = pkgs.symlinkJoin {
        name = "host";
        paths = [ config.packages.host-unwrapped ];
        nativeBuildInputs = [ pkgs.makeWrapper ];

        postBuild =
          let
            rootfsStore = ''$(find ${guestX86.config.system.build.argRootFs} -maxdepth 1 -type f \( -name '*.img' -o -name '*.ext4' \) | head -n1)'';
          in
          ''
            mkdir -p $out/libexec

            mv $out/bin/host $out/libexec/host-real

            wrapProgram $out/libexec/host-real \
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
              --set HOST_ASSET_DIR ${config.packages.host-unwrapped}/share/host/assets \
              --set XKB_CONFIG_ROOT ${pkgs.xkeyboard_config}/share/X11/xkb \
              --set ARGVM_CROSVM ${pkgs.lib.getExe pkgs.crosvm} \
              --set ARGVM_KERNEL ${guestDefs.x86_64.crosvm.kernelPath guestX86} \
              --set ARGVM_INITRD ${guestX86.config.system.build.initialRamdisk}/initrd \
              --set ARGVM_CONSOLE ${guestDefs.x86_64.crosvm.console}

            cat > $out/bin/host <<EOF
            #!${pkgs.bash}/bin/bash
            set -euo pipefail

            DATA_HOME="''${XDG_DATA_HOME:-\$HOME/.local/share}"
            APPDIR="\$DATA_HOME/ghost_in_a_shell"
            ROOTFS="\$APPDIR/rootfs.img"
            ROOTFS_STORE="${rootfsStore}"

            mkdir -p "\$APPDIR"

            if [ ! -e "\$ROOTFS" ]; then
              cp --reflink=never "\$ROOTFS_STORE" "\$ROOTFS"
              chmod 600 "\$ROOTFS"
            else
              chmod 600 "\$ROOTFS" || true
            fi

            export ARGVM_ROOTFS="\$ROOTFS"

            exec "$out/libexec/host-real" "\$@"
            EOF

            chmod +x $out/bin/host
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

        inherit host;
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
