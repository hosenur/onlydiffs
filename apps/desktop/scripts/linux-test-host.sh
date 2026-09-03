#!/usr/bin/env bash
# Stands up a throwaway Linux machine to point the SSH tests at.
#
# The local `sshd` in `tests/ssh.rs` proves the protocol; it cannot prove the
# thing most likely to break in the field, which is that a binary
# cross-compiled on macOS runs on someone's Debian build box. This gives that
# test a real one: a different kernel, a different libc, a different git, and a
# different architecture under emulation.
#
#   ./scripts/linux-test-host.sh up      # build, run, print the env to export
#   ./scripts/linux-test-host.sh down
set -euo pipefail

cd "$(dirname "$0")/.."
dir="${ONLYDIFFS_LINUX_HOST_DIR:-$PWD/.linux-test-host}"
name=onlydiffs-linux-test-host
port="${ONLYDIFFS_LINUX_HOST_PORT:-2223}"

case "${1:-up}" in
  up)
    mkdir -p "$dir"
    [ -f "$dir/client" ] || ssh-keygen -q -t ed25519 -f "$dir/client" -N "" -C onlydiffs-linux-test

    cat > "$dir/Dockerfile" <<'DOCKER'
FROM debian:bookworm-slim
# python3 is for the fake Claude channel the tests stand up on the host: it has
# to be a real process listening on real loopback, because that is the thing
# being proven.
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server git ca-certificates python3 \
 && rm -rf /var/lib/apt/lists/* && mkdir -p /run/sshd /root/.ssh && chmod 700 /root/.ssh
COPY client.pub /root/.ssh/authorized_keys
RUN chmod 600 /root/.ssh/authorized_keys \
 && sed -i 's/^#\?PermitRootLogin.*/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config \
 && git config --global user.email onlydiffs@example.test \
 && git config --global user.name "OnlyDiffs Test" \
 && git config --global init.defaultBranch main
CMD ["/usr/sbin/sshd", "-D", "-e"]
DOCKER

    # amd64 explicitly: the point is a platform this machine is not.
    docker build -q --platform linux/amd64 -t "$name" "$dir" >/dev/null
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker run -d --platform linux/amd64 --name "$name" -p "$port:22" "$name" >/dev/null

    for _ in $(seq 1 40); do
      ssh-keyscan -p "$port" -T 2 127.0.0.1 > "$dir/kh" 2>/dev/null && [ -s "$dir/kh" ] && break
      sleep 0.5
    done
    [ -s "$dir/kh" ] || { echo "the host never offered a key" >&2; exit 1; }

    echo "Linux host up on 127.0.0.1:$port. Run the test with:"
    echo "  ONLYDIFFS_LINUX_HOST_DIR=$dir ONLYDIFFS_LINUX_HOST_PORT=$port \\"
    echo "    cargo test --manifest-path src-tauri/Cargo.toml --test linux_host -- --nocapture"
    ;;
  down)
    docker rm -f "$name" >/dev/null 2>&1 || true
    echo "Linux host down."
    ;;
  *)
    echo "usage: $0 [up|down]" >&2
    exit 2
    ;;
esac
