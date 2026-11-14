#!/usr/bin/env python3
"""A STUN server for the NAT lab (tools/ci/natlab.sh): answers each Binding request (RFC 8489)
with the address it came from, as XOR-MAPPED-ADDRESS. IPv4 only.

    python3 stun_server.py 0.0.0.0 3478
"""
import socket
import struct
import sys

MAGIC = 0x2112A442


def answer(request, source):
    if len(request) < 20:
        return None
    kind, _length, magic = struct.unpack('!HHI', request[:8])
    if kind != 0x0001 or magic != MAGIC:
        return None
    transaction = request[8:20]
    ip, port = source
    xport = port ^ (MAGIC >> 16)
    xip = struct.unpack('!I', socket.inet_aton(ip))[0] ^ MAGIC
    value = struct.pack('!BBHI', 0, 1, xport, xip)
    attribute = struct.pack('!HH', 0x0020, len(value)) + value
    return struct.pack('!HHI', 0x0101, len(attribute), MAGIC) + transaction + attribute


def main():
    host, port = sys.argv[1], int(sys.argv[2])
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((host, port))
    print(f'stun: listening on {host}:{port}', flush=True)
    while True:
        data, source = s.recvfrom(2048)
        reply = answer(data, source)
        if reply:
            s.sendto(reply, source)


if __name__ == '__main__':
    main()
