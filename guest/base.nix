{ config, lib, pkgs, hostPkgs, modulesPath, ... }:

{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
    "${modulesPath}/profiles/qemu-guest.nix"
  ];
  networking.hostName = "argvm";
system.switch.enable = true;
environment.etc."init".source = "/nix/var/nix/profiles/system/init";

  boot = {
    loader = {
      grub.enable = false;
      systemd-boot.enable = false;
    };


  kernelParams = [
    "console=hvc0"
    "root=/dev/vda"
    "rw"
    "loglevel=4"
  ];

  initrd.availableKernelModules = [
    "virtio_blk"
    "virtio_pci"
    "virtio_net"
    "virtio_console"
    "overlay"
  ];

  supportedFilesystems = [
    "ext4"
    "overlay"
    "9p"
  ];

  # Keep the kernel fairly boring at first.
  kernelPackages = pkgs.linuxPackages;
  };

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



system.build.argRawImage = import "${hostPkgs.path}/nixos/lib/make-disk-image.nix" {
  inherit lib config;
  pkgs = hostPkgs;
  name = "argvm-riscv64-image";
  baseName = "argvm-riscv64";
  format = "raw";
  partitionTableType = "none";
  copyChannel = false;
  installBootLoader = true;
  diskSize = 8192;
};

  system.stateVersion = "25.11";
}
