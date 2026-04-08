{ writeShellApplication }:

writeShellApplication {
  name = "story-agent";
  text = ''
    set -euo pipefail
    mkdir -p /var/log
    echo "$(date -Iseconds) story-agent: boot observed" >> /var/log/story-agent.log
    while true; do
      sleep 30
      echo "$(date -Iseconds) story-agent: heartbeat" >> /var/log/story-agent.log
    done
  '';
}
