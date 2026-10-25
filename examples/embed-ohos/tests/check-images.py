#!/usr/bin/env python3
"""Check native image pixels, fade and surface isolation in the images fixture."""
import argparse
import importlib.util
from pathlib import Path
import struct
import tempfile
import time
import zlib

_spec = importlib.util.spec_from_file_location("surfaces", Path(__file__).with_name("check-surfaces.py"))
_surfaces = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_surfaces)


def pixels(path):
    """Decode the emulator's eight-bit RGB/RGBA PNG without third-party tools."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed = 8, bytearray()
    while offset < len(data):
        size, kind = struct.unpack_from(">I4s", data, offset)
        block = data[offset + 8:offset + 8 + size]
        offset += size + 12
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", block)
            assert depth == 8 and color in (2, 6) and interlace == 0
        elif kind == b"IDAT":
            compressed.extend(block)
    channels = 4 if color == 6 else 3
    stride = width * channels
    raw = zlib.decompress(compressed)
    previous = bytearray(stride)
    rows = []
    for y in range(height):
        base = y * (stride + 1)
        filter_kind = raw[base]
        row = bytearray(raw[base + 1:base + stride + 1])
        for x in range(stride) if filter_kind else ():
            a = row[x - channels] if x >= channels else 0
            b = previous[x]
            c = previous[x - channels] if x >= channels else 0
            if filter_kind == 1:
                predictor = a
            elif filter_kind == 2:
                predictor = b
            elif filter_kind == 3:
                predictor = (a + b) // 2
            elif filter_kind == 4:
                p = a + b - c
                distances = (abs(p - a), abs(p - b), abs(p - c))
                predictor = (a, b, c)[distances.index(min(distances))]
            else:
                assert filter_kind == 0
                predictor = 0
            row[x] = (row[x] + predictor) & 255
        rows.append(row)
        previous = row
    return lambda x, y: tuple(rows[y][x * channels:x * channels + 3])


def check(device, directory):
    def capture(name):
        remote = "/data/local/tmp/nsh-images-check.png"
        local = directory / (name + ".png")
        device.run("shell", "uitest", "screenCap", "-p", remote)
        device.run("file", "recv", remote, str(local))
        return pixels(local)

    def close(actual, expected):
        assert max(abs(a - b) for a, b in zip(actual, expected)) <= 6, (actual, expected)

    def center(bounds):
        l, t, r, b = bounds
        return (l + r) // 2, (t + b) // 2

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    device.counts([1, 1], "images-loaded:")
    time.sleep(2.5)
    fade = list(map(device.bounds, device.matching("image-fade")))
    blur = list(map(device.bounds, device.matching("image-blur")))
    p = capture("initial")
    for bounds in fade:
        close(p(*center(bounds)), (34, 197, 94))
    for bounds in blur:
        close(p(*center(bounds)), (239, 68, 68))
    device.click(device.matching("image-tint-toggle")[0])
    p = capture("tinted")
    close(p(*center(fade[0])), (37, 99, 235))
    close(p(*center(fade[1])), (34, 197, 94))
    # Tint preserves the source's transparent margin. Blur softens that edge.
    l, t, r, b = fade[0]
    close(p(l + (r - l) // 16, (t + b) // 2), (255, 255, 255))
    l, t, r, b = blur[0]
    softened = p(l + (r - l) // 8, (t + b) // 2)
    assert 90 < softened[1] < 230, softened
    device.click(device.matching("image-remount")[0])
    device.counts([2, 1], "images-loaded:")
    early = capture("fade-early")(*center(fade[0]))
    assert 45 < early[0] < 245, early
    time.sleep(2.3)
    final = capture("fade-final")
    close(final(*center(fade[0])), (37, 99, 235))
    close(final(*center(fade[1])), (34, 197, 94))
    print("PASS: native image tint/alpha, blur, timed fade, remount and surface isolation")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.output:
        args.output.mkdir(parents=True, exist_ok=True)
        check(_surfaces.Device(args.target, args.output), args.output)
    else:
        with tempfile.TemporaryDirectory(prefix="nsh-images-") as tmp:
            check(_surfaces.Device(args.target, Path(tmp)), Path(tmp))
