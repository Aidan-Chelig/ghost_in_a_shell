{ config, lib, pkgs, modulesPath, ... }:

{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
  ];

  networking.hostName = "argvm";

  boot.loader.grub.enable = false;
  boot.loader.systemd-boot.enable = false;

  boot.kernelParams = [
    "console=hvc0"
    "root=/dev/vda"
    "rw"
    "loglevel=4"
  ];

  boot.initrd.availableKernelModules = [
    "virtio_blk"
    "virtio_pci"
    "virtio_net"
    "virtio_console"
    "overlay"
  ];

  boot.supportedFilesystems = [
    "ext4"
    "overlay"
    "9p"
  ];

  # Keep the kernel fairly boring at first.
  boot.kernelPackages = pkgs.linuxPackages;

  fileSystems."/" = {
    device = "/dev/vda";
    fsType = "ext4";
    autoResize = true;
  };

  networking.useDHCP = true;

  services.getty.autologinUser = "root";

  users.users.root = {
    initialPassword = "root";
  };

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

    (callPackage ../pkgs/story-agent.nix { })
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

  # Helpful for a terminal-first game box.
  programs.bash.interactiveShellInit = ''
    export PS1="[\u@argvm \w]\\$ "
    echo
    echo "ARGVM boot complete."
    echo "Type 'journalctl -b' or inspect /var/log."
    echo
  '';

  # Raw image for qemu/crosvm.
  image.fileName = "argvm-riscv64.raw";

  system.build.argRawImage = import "${pkgs.path}/nixos/lib/make-disk-image.nix" {
    inherit lib config pkgs;
    format = "raw";
    partitionTableType = "none";
    copyChannel = false;
    compressImage = false;
    diskSize = 2048;
  };

  system.stateVersion = "25.11";
}
