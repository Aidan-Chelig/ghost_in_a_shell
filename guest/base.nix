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
    attr
    (callPackage ./pkgs/story-agent.nix { })
    (callPackage ./pkgs/ghost-agent.nix { })
  ];

systemd.tmpfiles.rules = [
  "d /run/ghost 0755 root root -"
  "d /root/world 0755 root root -"
  "d /root/world/office 0755 root root -"
  "f /run/ghost/current-cwd 0644 root root -"
];

environment.etc."ghost-seed-world.sh".text = ''
  mkdir -p /root/world/office
  echo "Something is wrong with this machine." > /root/world/office/note.txt || true
'';

  systemd.services.story-agent = {
    description = "ARG story agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.callPackage ./pkgs/story-agent.nix { }}/bin/story-agent";
      Restart = "always";
      RestartSec = 2;
    };
  };

systemd.services.ghost-seed-world = {
  description = "Seed ghost world";
  wantedBy = [ "multi-user.target" ];
  before = [ "ghost-agent.service" ];
  serviceConfig = {
    Type = "oneshot";
    ExecStart = "${pkgs.writeShellScript "ghost-seed-world" ''
      mkdir -p /root/world/office
      if [ ! -e /root/world/office/note.txt ]; then
        echo "Something is wrong with this machine." > /root/world/office/note.txt
      fi
      ${pkgs.attr}/bin/setfattr -n user.ghost.kind -v room /root/world/office || true
      ${pkgs.attr}/bin/setfattr -n user.ghost.label -v Office /root/world/office || true
      ${pkgs.attr}/bin/setfattr -n user.ghost.kind -v note /root/world/office/note.txt || true
      ${pkgs.attr}/bin/setfattr -n user.ghost.label -v "Pinned Note" /root/world/office/note.txt || true
    ''}";
  };
};

systemd.services.ghost-agent = {
  description = "Ghost host communication agent";
  wantedBy = [ "multi-user.target" ];
  after = [ "network.target" ];
  serviceConfig = {
    ExecStart = "${pkgs.callPackage ./pkgs/ghost-agent.nix { }}/bin/ghost-agent";
    Restart = "always";
    RestartSec = 2;
  };
};

programs.bash.interactiveShellInit = ''
  export PS1="[\u@argvm \w]\\$ "
  echo
  echo "ARGVM boot complete (${guestSystem})."
  echo

  __ghost_last_pwd=""
  __ghost_emit_cwd() {
    if [ "$PWD" != "$__ghost_last_pwd" ]; then
      mkdir -p /run/ghost
      printf '%s\n' "$PWD" > /run/ghost/current-cwd.tmp
      mv /run/ghost/current-cwd.tmp /run/ghost/current-cwd
      __ghost_last_pwd="$PWD"
    fi
  }

  case ";$PROMPT_COMMAND;" in
    *";__ghost_emit_cwd;"*) ;;
    *)
    PROMPT_COMMAND="__ghost_emit_cwd''${PROMPT_COMMAND:+;''$PROMPT_COMMAND}"
      ;;
  esac
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
