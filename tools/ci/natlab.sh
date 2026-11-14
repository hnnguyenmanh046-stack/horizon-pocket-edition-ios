#!/usr/bin/env bash
# The NAT lab: a shared world's host and guest on two simulated home networks, each behind its
# own router (Linux NAT in a network namespace), seven hops apart across a simulated internet
# (a chain of five routers) with a real bore server and a STUN server in its middle. Each
# scenario checks how the guest's connection goes:
#
#   forwarded  the host's router forwards its port:            direct TCP, no tunnel, no UDP
#   udp        cone NATs that drop every inbound TCP SYN:      TCP fails, UDP carries the game
#   symmetric  NATs with a new port per destination, no TCP:   the tunnel (bore)
#   cone       cone NATs, TCP allowed:                         direct (TCP if it meets, else UDP)
#
#   hostA 10.1.0.2 - natA 192.0.2.1 - r1 - r2 - r3 - r4 - r5 - natB 198.51.100.1 - hostB 10.2.0.2
#                                              |
#                                     svc 203.0.113.10 (bore, STUN)
#
#   sudo -E bash tools/ci/natlab.sh path/to/natlab path/to/bore
set -euo pipefail
natlab=$(realpath "$1")
bore=$(realpath "$2")
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
SPACES="svc r1 r2 r3 r4 r5 natA natB hostA hostB"
nsx() { ip netns exec "$@"; }

cleanup() {
  for n in $SPACES; do ip netns del "$n" 2>/dev/null || true; done
}

# A link between namespaces $1 and $3: interface $2 in $1 with address $5, $4 in $3 with $6.
link() {
  ip link add "$2" type veth peer name "$4"
  ip link set "$2" netns "$1"
  ip link set "$4" netns "$3"
  nsx "$1" ip addr add "$5" dev "$2"
  nsx "$3" ip addr add "$6" dev "$4"
  nsx "$1" ip link set "$2" up
  nsx "$3" ip link set "$4" up
}

# $1, $2: the NAT kind of A's and of B's router (cone | symmetric); $3: 1 drops inbound TCP
# SYNs at both routers; $4: 1 forwards the host's TCP port on A's router.
setup() {
  cleanup
  for n in $SPACES; do
    ip netns add "$n"
    nsx "$n" ip link set lo up
    nsx "$n" sysctl -qw net.ipv6.conf.all.disable_ipv6=1
    nsx "$n" sysctl -qw net.ipv4.ip_forward=1
  done
  # The internet: r1 .. r5, the services beside r3.
  link r1 c12 r2 c21 10.255.12.1/24 10.255.12.2/24
  link r2 c23 r3 c32 10.255.23.1/24 10.255.23.2/24
  link r3 c34 r4 c43 10.255.34.1/24 10.255.34.2/24
  link r4 c45 r5 c54 10.255.45.1/24 10.255.45.2/24
  link svc s0 r3 s3 203.0.113.10/24 203.0.113.254/24
  link natA wanA r1 a1 192.0.2.1/24 192.0.2.254/24
  link natB wanB r5 b5 198.51.100.1/24 198.51.100.254/24
  nsx r1 ip route add default via 10.255.12.2
  nsx r2 ip route add 192.0.2.0/24 via 10.255.12.1
  nsx r2 ip route add default via 10.255.23.2
  nsx r3 ip route add 192.0.2.0/24 via 10.255.23.1
  nsx r3 ip route add 198.51.100.0/24 via 10.255.34.2
  nsx r4 ip route add 198.51.100.0/24 via 10.255.45.2
  nsx r4 ip route add default via 10.255.34.1
  nsx r5 ip route add default via 10.255.45.1
  nsx svc ip route add default via 203.0.113.254
  # The two homes.
  link natA lanA hostA ethA 10.1.0.1/24 10.1.0.2/24
  link natB lanB hostB ethB 10.2.0.1/24 10.2.0.2/24
  nsx natA ip route add default via 192.0.2.254
  nsx natB ip route add default via 198.51.100.254
  nsx hostA ip route add default via 10.1.0.1
  nsx hostB ip route add default via 10.2.0.1
  local side kind
  for side in A B; do
    if [ "$side" = A ]; then kind=$1; else kind=$2; fi
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
  nsx svc python3 "$here/stun_server.py" 0.0.0.0 3478 > "$work/stun.log" 2>&1 &
  nsx svc "$bore" server --min-port 20000 --max-port 30000 > "$work/bore.log" 2>&1 &
  sleep 1
  # The internet works: each home reaches the services, and natA's address reaches natB's.
  nsx hostA ping -c1 -W2 203.0.113.10 > /dev/null
  nsx hostB ping -c1 -W2 203.0.113.10 > /dev/null
  nsx natA ping -c1 -W2 198.51.100.1 > /dev/null
}

# $1: the scenario; $2: what the guest's connection must be (direct-tcp | udp | tunnel | direct).
run() {
  local name=$1 want=$2 out="$work/$1"
  mkdir -p "$out"
  echo "== $name"
  nsx hostA env HORIZON_FRIENDS_TRACE=1 "$natlab" host --bore 203.0.113.10 --stun 203.0.113.10:3478 --out "$out/code" \
    > "$out/host.log" 2> "$out/host.err" &
  local host=$!
  for _ in $(seq 1 300); do [ -s "$out/code" ] && break; sleep 0.1; done
  [ -s "$out/code" ] || { cat "$out/host.err"; echo "$name: the host shared nothing"; return 1; }
  local result
  if ! result=$(timeout 90 ip netns exec hostB env HORIZON_FRIENDS_TRACE=1 "$natlab" guest --code "$out/code" --stun 203.0.113.10:3478 2> "$out/guest.err"); then
    cat "$out/host.err" "$out/guest.err"; echo "$name: the guest didn't join"; kill "$host" || true; return 1
  fi
  sleep 0.5
  kill "$host" 2>/dev/null || true
  wait "$host" 2>/dev/null || true
  echo "guest: $result"
  echo "host:  $(cat "$out/host.log")"
  grep -h "horizon-friends:" "$out/host.err" "$out/guest.err" | sed "s/^/  /" || true
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

stop_services() {
  pkill -f stun_server.py || true
  pkill -f "$bore server" || true
}
trap 'stop_services; cleanup' EXIT
status=0
scenario() {
  local name=$1 a=$2 b=$3 block=$4 forward=$5 want=$6
  setup "$a" "$b" "$block" "$forward"
  run "$name" "$want" || status=1
  stop_services
  sleep 0.5
}
scenario forwarded cone cone 0 1 direct-tcp
scenario udp cone cone 1 0 udp
scenario symmetric symmetric symmetric 1 0 tunnel
scenario cone cone cone 0 0 direct
exit $status
