#!/usr/bin/env python3
"""Mac AVD multiplayer discovery bridge. Python stdlib only; no ROM or save access.

Android Emulator NAT does not forward mDNS multicast, so an Android AVD cannot
see the KONKR Host's original-slot DNS-SD advertisement. macOS *can* see it.
We join _slotlink._tcp.local on the host and forward ONLY a validated LAN IPv4
via an ADB-reversed loopback TCP socket. The existing strict ROM/core/BIOS
KMLB/2 handshake remains entirely inside Slot Android; this bridge sends no
game bytes and is NEVER a replacement for that handshake.

Usage (Mac Terminal):
  python3 scripts/slot-kpa-mac-link-bridge.py --serial emulator-5556
Leave running; Host on KONKR, then SELECT+HOME -> Join on Mac. If a host is not
found within the bounded discovery window, the familiar manual IP dialog opens.
"""
from __future__ import annotations

import argparse
import ipaddress
import os
from pathlib import Path
import socket
import socketserver
import struct
import subprocess
import sys
import threading
import time

SERVICE = "_slotlink._tcp.local"
GROUP = "224.0.0.251"
MDNS_PORT = 5353
LINK_PORT = 7211
BRIDGE_PORT = 7222
CACHE_SECONDS = 20
FIND_SECONDS = 7
# Wait briefly after the first advertisement for other hosts on the same game.
# Otherwise the first packet can win before another console is discovered.
SETTLE_SECONDS = 0.7
MAX_PACKET = 1500


def dns_name(packet: bytes, at: int) -> tuple[str, int]:
    parts: list[str] = []
    end = None
    jumps = 0
    while True:
        if at >= len(packet):
            raise ValueError("truncated name")
        n = packet[at]
        at += 1
        if n == 0:
            return ".".join(parts).lower(), (at if end is None else end)
        if n & 0xC0 == 0xC0:
            if at >= len(packet):
                raise ValueError("truncated pointer")
            target = ((n & 0x3F) << 8) | packet[at]
            at += 1
            if end is None:
                end = at
            jumps += 1
            if jumps > 16 or target >= len(packet):
                raise ValueError("name pointer loop")
            at = target
            continue
        if n & 0xC0 or n > 63 or at + n > len(packet):
            raise ValueError("invalid label")
        parts.append(packet[at:at + n].decode("ascii", errors="replace"))
        at += n
        if sum(map(len, parts)) > 255:
            raise ValueError("name too long")


def put_name(name: str) -> bytes:
    return b"".join(bytes([len(label)]) + label.encode("ascii")
                    for label in name.split(".")) + b"\0"


def mdns_query() -> bytes:
    return (struct.pack("!6H", 0, 0, 1, 0, 0, 0)
            + put_name(SERVICE) + struct.pack("!HH", 12, 1))


def valid_host_ip(text: str) -> bool:
    try:
        ip = ipaddress.IPv4Address(text)
        return (ip.is_private and not ip.is_loopback and
                not ip.is_link_local and not ip.is_multicast and
                not ip.is_unspecified and not ip.is_reserved)
    except ipaddress.AddressValueError:
        return False


def parse_mdns_hosts(packet: bytes) -> list[tuple[str, str]]:
    """Original slot/pvaibhav DNS-SD: complete PTR+SRV+TXT+A in one packet.

    Discard partial, malformed, goodbye, non-Slot and incompatible v records.
    This is discovery only; Slot's native handshake separately validates ROM,
    core, BIOS, backend and GBA Cable/RFU mode on the real TCP connection.
    """
    try:
        if len(packet) < 12:
            return []
        _, flags, questions, answers, authority, additional = struct.unpack_from("!6H", packet)
        if not flags & 0x8000 or questions > 40 or answers + authority + additional > 80:
            return []
        at = 12
        for _ in range(questions):
            _, at = dns_name(packet, at)
            if at + 4 > len(packet):
                return []
            at += 4
        records = []
        for _ in range(answers + authority + additional):
            name, at = dns_name(packet, at)
            if at + 10 > len(packet):
                return []
            kind, _cls, ttl, n = struct.unpack_from("!HHIH", packet, at)
            at += 10
            if n > MAX_PACKET or at + n > len(packet):
                return []
            records.append((name, kind, ttl, at, packet[at:at + n]))
            at += n
        matches = []
        for name, kind, ttl, off, data in records:
            if name != SERVICE or kind != 12 or ttl == 0:
                continue
            instance, _ = dns_name(packet, off)
            if not instance.endswith("." + SERVICE):
                continue
            srv = next((r for r in records if r[0] == instance and r[1] == 33 and r[2] > 0), None)
            txt = next((r for r in records if r[0] == instance and r[1] == 16 and r[2] > 0), None)
            if srv is None or txt is None or len(srv[4]) < 7:
                continue
            port = struct.unpack_from("!H", srv[4], 4)[0]
            if port != LINK_PORT:
                continue
            target, _ = dns_name(packet, srv[3] + 6)
            addr = next((r[4] for r in records
                         if r[0] == target and r[1] == 1 and r[2] > 0 and len(r[4]) == 4), None)
            if addr is None:
                continue
            fields: dict[str, str] = {}
            pos = 0
            while pos < len(txt[4]):
                size = txt[4][pos]
                pos += 1
                field = txt[4][pos:pos + size]
                if len(field) != size:
                    raise ValueError("malformed TXT")
                pos += size
                item = field.decode("ascii", errors="replace")
                if "=" in item:
                    key, value = item.split("=", 1)
                    fields[key] = value
            ip = socket.inet_ntoa(addr)
            game = fields.get("game", "")
            if (fields.get("v") == "1" and
                    len(game) == 4 and game.isascii() and game.isalnum() and
                    game.upper() == game and valid_host_ip(ip)):
                matches.append((game, ip))
        return matches
    except (ValueError, UnicodeError, IndexError, struct.error, OSError):
        return []


class Browser:
    def __init__(self):
        self.lock = threading.Lock()
        # A game code is not a unique peer identity. Never silently choose
        # the most recently advertised IP when two consoles host that game.
        self.hosts: dict[str, dict[str, float]] = {}
        self.running = True
        self.sock: socket.socket | None = None

    def remember(self, game: str, ip: str, when: float | None = None):
        if not valid_host_ip(ip):
            return
        now = time.monotonic() if when is None else when
        with self.lock:
            self.hosts.setdefault(game, {})[ip] = now

    def candidates(self, game: str, when: float | None = None) -> list[str]:
        now = time.monotonic() if when is None else when
        with self.lock:
            entries = self.hosts.get(game, {})
            for ip, seen in list(entries.items()):
                if now - seen > CACHE_SECONDS:
                    del entries[ip]
            if not entries:
                self.hosts.pop(game, None)
            return sorted(entries)

    def find(self, game: str, seconds: float = FIND_SECONDS) -> str | None:
        deadline = time.monotonic() + seconds
        first_seen: float | None = None
        chosen: str | None = None
        while time.monotonic() < deadline and self.running:
            hosts = self.candidates(game)
            if len(hosts) > 1:
                # Fail closed: the existing manual-IP dialog lets the user
                # choose the correct KONKR instead of a random same-game host.
                print(f"Multiple Slot hosts for {game}; use Direct Join", flush=True)
                return None
            if hosts:
                if chosen != hosts[0]:
                    chosen, first_seen = hosts[0], time.monotonic()
                elif first_seen is not None and time.monotonic() - first_seen >= SETTLE_SECONDS:
                    return chosen
            else:
                chosen, first_seen = None, None
            self.ask()
            time.sleep(0.15)
        return None

    def ask(self):
        if self.sock is not None:
            try:
                self.sock.sendto(mdns_query(), (GROUP, MDNS_PORT))
            except OSError:
                pass

    def browse(self):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
        try:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            if hasattr(socket, "SO_REUSEPORT"):
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
            sock.bind(("", MDNS_PORT))
            membership = socket.inet_aton(GROUP) + socket.inet_aton("0.0.0.0")
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, membership)
            sock.settimeout(1.0)
            self.sock = sock
            print("mDNS: browsing _slotlink._tcp.local (original Slot format)", flush=True)
            self.ask()
            while self.running:
                try:
                    packet, _from = sock.recvfrom(2048)
                except socket.timeout:
                    continue
                for game, ip in parse_mdns_hosts(packet):
                    self.remember(game, ip)
        except OSError as error:
            print(f"mDNS unavailable: {error}; manual Join remains available", file=sys.stderr)
        finally:
            self.sock = None
            sock.close()


class Bridge(socketserver.ThreadingMixIn, socketserver.TCPServer):
    allow_reuse_address = True
    daemon_threads = True
    def __init__(self, browser: Browser, port: int):
        self.browser = browser
        super().__init__(("127.0.0.1", port), BridgeHandler)


class BridgeHandler(socketserver.StreamRequestHandler):
    def handle(self):
        self.request.settimeout(FIND_SECONDS + 2)
        line = self.rfile.readline(64).decode("ascii", errors="replace").strip()
        if not line.startswith("FIND ") or len(line) != 9:
            self.wfile.write(b"NONE\n")
            return
        game = line[5:]
        if not game.isalnum() or game.upper() != game:
            self.wfile.write(b"NONE\n")
            return
        ip = self.server.browser.find(game)
        self.wfile.write((ip if ip else "NONE").encode("ascii") + b"\n")


def self_test():
    assert mdns_query().endswith(put_name(SERVICE) + struct.pack("!HH", 12, 1))
    assert valid_host_ip("192.168.68.79") and not valid_host_ip("127.0.0.1")
    def rr(name, kind, ttl, data):
        return put_name(name) + struct.pack("!HHIH", kind, 1, ttl, len(data)) + data
    instance = "slot-test." + SERVICE
    target = "slot-test.local"
    txt = b"\x03v=1\x09game=BPRE"
    packet = (struct.pack("!6H", 0, 0x8400, 0, 1, 0, 3) +
              rr(SERVICE, 12, 120, put_name(instance)) +
              rr(instance, 33, 120, b"\x00\x00\x00\x00" +
                 struct.pack("!H", LINK_PORT) + put_name(target)) +
              rr(instance, 16, 120, txt) +
              rr(target, 1, 120, socket.inet_aton("192.168.68.79")))
    assert parse_mdns_hosts(packet) == [("BPRE", "192.168.68.79")]
    assert parse_mdns_hosts(packet[:18]) == []
    assert parse_mdns_hosts(packet.replace(b"v=1", b"v=9")) == []
    assert parse_mdns_hosts(packet.replace(b"\x00\x00\x00\x78", b"\x00\x00\x00\x00")) == []
    browser = Browser()
    now = time.monotonic()
    browser.remember("BPRE", "192.168.68.79", now)
    assert browser.candidates("BPRE", now) == ["192.168.68.79"]
    browser.remember("BPRE", "192.168.68.80", now)
    assert browser.candidates("BPRE", now) == ["192.168.68.79", "192.168.68.80"]
    # Multiple discoverable hosts with the same four-letter game code MUST
    # fall back to explicit IP; no last-write-wins and no arbitrary choice.
    assert browser.find("BPRE", seconds=0.01) is None
    browser.remember("BPRE", "192.168.68.80", now - CACHE_SECONDS - 1)
    assert browser.candidates("BPRE", now) == ["192.168.68.79"]
    assert browser.candidates("BPRE", now + CACHE_SECONDS + 1) == []
    browser.remember("BPRE", "192.168.68.79")
    with Bridge(browser, 0) as srv:
        t = threading.Thread(target=srv.serve_forever, daemon=True)
        t.start()
        with socket.create_connection(srv.server_address, timeout=2) as conn:
            conn.sendall(b"FIND BPRE\n")
            assert conn.recv(64) == b"192.168.68.79\n"
        browser.remember("BPRE", "192.168.68.80")
        with socket.create_connection(srv.server_address, timeout=2) as conn:
            conn.sendall(b"FIND BPRE\n")
            assert conn.recv(64) == b"NONE\n"
        srv.shutdown()
    browser.running = False
    print("Mac Link bridge protocol + mDNS self-test: PASS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", default="emulator-5556")
    parser.add_argument("--adb", default=str(Path.home() / "Library/Android/sdk/platform-tools/adb"))
    parser.add_argument("--port", type=int, default=BRIDGE_PORT)
    parser.add_argument("--self-test", action="store_true")
    opts = parser.parse_args()
    if opts.self_test:
        self_test()
        return
    if not opts.serial.startswith("emulator-"):
        parser.error("Mac bridge may forward only to an Android emulator serial")
    browser = Browser()
    threading.Thread(target=browser.browse, name="slot-mdns-browse", daemon=True).start()
    with Bridge(browser, opts.port) as server:
        # On Android, 127.0.0.1:<port> now reaches only this Mac loopback server.
        # The service never binds a LAN port or exposes files or emulator controls.
        adb = subprocess.run([opts.adb, "-s", opts.serial, "reverse",
                              f"tcp:{opts.port}", f"tcp:{opts.port}"],
                             capture_output=True, text=True)
        if adb.returncode:
            raise SystemExit(f"ADB reverse failed: {adb.stderr.strip()}\n"
                             "Start the AVD first; IP entry still works without bridge.")
        print(f"Link bridge ready: {opts.serial} ↔ macOS Bonjour, "
              f"127.0.0.1:{opts.port}; Ctrl+C to stop", flush=True)
        try:
            server.serve_forever(poll_interval=0.5)
        except KeyboardInterrupt:
            pass
        finally:
            browser.running = False
            subprocess.run([opts.adb, "-s", opts.serial, "reverse", "--remove",
                            f"tcp:{opts.port}"], capture_output=True)


if __name__ == "__main__":
    main()
