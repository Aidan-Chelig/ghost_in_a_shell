{ config, lib, pkgs, hostPkgs, guestSystem, modulesPath, ... }:

let
  isX86 = guestSystem == "x86_64-linux";
  isRiscv = guestSystem == "riscv64-linux";

  console =
    if isX86 then "ttyS0"
    else if isRiscv then "ttyS0"
    else "ttyS0";

  qemuGuestImport =
    if isX86
    then "${modulesPath}/profiles/qemu-guest.nix"
    else "${modulesPath}/profiles/qemu-guest.nix";
in
{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
    qemuGuestImport
  ];

  networking.hostName = "argvm";

  boot.loader.grub.enable = false;
  boot.loader.systemd-boot.enable = false;

  boot.kernelParams = [
    "console=${console}"
    "root=/dev/vda"
    "rw"
    "loglevel=7"
  ];

  boot.initrd.availableKernelModules = [
    "virtio_blk"
    "virtio_pci"
    "virtio_net"
    "virtio_console"
    "overlay"
  ];

  boot.supportedFilesystems = [ "ext4" "overlay" "9p" ];

  fileSystems."/" = {
    device = "/dev/vda";
    fsType = "ext4";
    autoResize = false;
  };

  networking.useDHCP = true;

  services.getty.autologinUser = "root";
  systemd.services."serial-getty@${console}".enable = true;

  users.users.root.initialPassword = "root";

  environment.systemPackages = with pkgs; [
    busybox
    coreutils
    curl
    dnsutils
    file
    findutils
    gawk
    gnugrep
    gnused
    iproute2
    iputils
    less
    procps
    strace
    tree
    vim
    which
    (callPackage ./pkgs/story-agent.nix { })
  ];

  systemd.services.story-agent = {
    description = "ARG story agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.callPackage ../pkgs/story-agent.nix { }}/bin/story-agent";
      Restart = "always";
      RestartSec = 2;
    };
  };

  programs.bash.interactiveShellInit = ''
    export PS1="[\u@argvm \w]\\$ "
    echo
    echo "ARGVM boot complete (${guestSystem})."
    echo
  '';

  system.build.argRootFs =
    hostPkgs.callPackage "${hostPkgs.path}/nixos/lib/make-ext4-fs.nix" {
      storePaths = [ config.system.build.toplevel ];
      volumeLabel = "ARGROOT";
      populateImageCommands = ''
        mkdir -p ./files/nix/var/nix/profiles
        mkdir -p ./files/etc

        ln -s ${config.system.build.toplevel} ./files/nix/var/nix/profiles/system
        ln -s /nix/var/nix/profiles/system/init ./files/init

        cat > ./files/etc/hostname <<'EOF'
        argvm
        EOF
      '';
    };

  system.stateVersion = "25.11";
}
