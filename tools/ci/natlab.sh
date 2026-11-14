#!/usr/bin/env bash
# The NAT lab: a shared world's host and guest on two simulated home networks, each behind its
# own router (Linux NAT in a network namespace), with a real bore server and a STUN server on
# the simulated internet between them. Each scenario checks how the guest's connection goes:
#
#   forwarded  the host's router forwards its port:            direct TCP, no tunnel, no UDP
#   udp        cone NATs that drop every inbound TCP SYN:      TCP fails, UDP carries the game
#   symmetric  NATs with a new port per destination, no TCP:   bore.pub-style tunnel
#   cone       cone NATs, TCP allowed:                         direct (TCP if it meets, else UDP)
#
#   sudo -E bash tools/ci/natlab.sh path/to/natlab path/to/bore
set -euo pipefail
natlab=$(realpath "$1")
bore=$(realpath "$2")
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
nsx() { ip netns exec "$@"; }

cleanup() {
  for n in inet natA natB hostA hostB; do ip netns del "$n" 2>/dev/null || true; done
  for l in upA upB wanA wanB lanA lanB ethA ethB; do ip link del "$l" 2>/dev/null || true; done
}

# $1, $2: the NAT kind of A's and of B's router (cone | symmetric); $3: 1 drops inbound TCP
# SYNs at both routers; $4: 1 forwards the host's TCP port on A's router.
setup() {
  cleanup
  for n in inet natA natB hostA hostB; do
    ip netns add "$n"
    nsx "$n" ip link set lo up
    nsx "$n" sysctl -qw net.ipv6.conf.all.disable_ipv6=1
  done
  nsx inet ip link add br0 type bridge
  nsx inet ip addr add 198.51.100.254/24 dev br0
  nsx inet ip link set br0 up
  local i=0 side kind
  for side in A B; do
    i=$((i + 1))
    if [ "$side" = A ]; then kind=$1; else kind=$2; fi
    ip link add "wan$side" type veth peer name "up$side"
    ip link set "wan$side" netns "nat$side"
    ip link set "up$side" netns inet
    nsx inet ip link set "up$side" master br0 up
    nsx "nat$side" ip addr add "198.51.100.$i/24" dev "wan$side"
    nsx "nat$side" ip link set "wan$side" up
    ip link add "lan$side" type veth peer name "eth$side"
    ip link set "lan$side" netns "nat$side"
    ip link set "eth$side" netns "host$side"
    nsx "nat$side" ip addr add "10.$i.0.1/24" dev "lan$side"
    nsx "nat$side" ip link set "lan$side" up
    nsx "host$side" ip addr add "10.$i.0.2/24" dev "eth$side"
    nsx "host$side" ip link set "eth$side" up
    nsx "host$side" ip route add default via "10.$i.0.1"
    nsx "nat$side" sysctl -qw net.ipv4.ip_forward=1
    if [ "$kind" = symmetric ]; then
      nsx "nat$side" iptables -t nat -A POSTROUTING -o "wan$side" -j MASQUERADE --random-fully
    else
      nsx "nat$side" iptables -t nat -A POSTROUTING -o "wan$side" -j MASQUERADE
    fi
    if [ "$3" = 1 ]; then
      nsx "nat$side" iptables -A FORWARD -i "wan$side" -p tcp --syn -j DROP
    fi
  done
  if [ "$4" = 1 ]; then
    nsx natA iptables -t nat -A PREROUTING -i wanA -p tcp --dport 25580 -j DNAT --to-destination 10.1.0.2:25580
  fi
  nsx inet python3 "$here/stun_server.py" 0.0.0.0 3478 > "$work/stun.log" 2>&1 &
  nsx inet "$bore" server --min-port 20000 --max-port 30000 > "$work/bore.log" 2>&1 &
  sleep 1
}

# $1: the scenario; $2: what the guest's connection must be (direct-tcp | udp | tunnel | direct).
run() {
  local name=$1 want=$2 out="$work/$1"
  mkdir -p "$out"
  echo "== $name"
  nsx hostA "$natlab" host --bore 198.51.100.254 --stun 198.51.100.254:3478 --out "$out/code" \
    > "$out/host.log" 2> "$out/host.err" &
  local host=$!
  for _ in $(seq 1 300); do [ -s "$out/code" ] && break; sleep 0.1; done
  [ -s "$out/code" ] || { cat "$out/host.err"; echo "$name: the host shared nothing"; return 1; }
  local result
  if ! result=$(timeout 90 ip netns exec hostB "$natlab" guest --code "$out/code" --stun 198.51.100.254:3478 2> "$out/guest.err"); then
    cat "$out/host.err" "$out/guest.err"; echo "$name: the guest didn't join"; kill "$host" || true; return 1
  fi
  sleep 0.5
  kill "$host" 2>/dev/null || true
  wait "$host" 2>/dev/null || true
  echo "guest: $result"
  echo "host:  $(cat "$out/host.log")"
  python3 - "$want" "$result" <<'PY'
import json, sys
want, got = sys.argv[1], json.loads(sys.argv[2])
ok = {
    'direct-tcp': got['direct'] and not got['udp'] and not got['tunneled'],
    'udp': got['direct'] and got['udp'] and not got['tunneled'],
    'tunnel': got['tunneled'] and not got['direct'] and not got['udp'],
    'direct': got['direct'] and not got['tunneled'],
}[want]
assert ok, f'expected {want}, got {got}'
assert got['join_ms'] < 20000, got
PY
}

trap 'cleanup; pkill -f stun_server.py || true; pkill -f "$bore server" || true' EXIT
status=0
scenario() {
  local name=$1 a=$2 b=$3 block=$4 forward=$5 want=$6
  setup "$a" "$b" "$block" "$forward"
  run "$name" "$want" || status=1
  pkill -f stun_server.py || true
  pkill -f "$bore server" || true
  sleep 0.5
}
scenario forwarded cone cone 0 1 direct-tcp
scenario udp cone cone 1 0 udp
scenario symmetric symmetric symmetric 1 0 tunnel
scenario cone cone cone 0 0 direct
exit $status
