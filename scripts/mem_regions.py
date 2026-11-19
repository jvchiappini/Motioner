#!/usr/bin/env python3
"""Attribute a Windows process's committed memory (prompt 18 P3).

Walks the target process address space with ``VirtualQueryEx`` and groups the
committed bytes by kind: private (heap, Rust allocations, driver staging),
mapped files and images.  For private regions it reports the largest blocks and
a histogram by protection, so an owner can be found before touching code.  For
mapped or image regions it reports the backing file.

This is a read-only instrument: it never writes to the target and it is not in
the gate.

Usage::

    python scripts/mem_regions.py --launch "<exe> --start --hidden" --delay 6
    python scripts/mem_regions.py --pid 1234 --json out.json
"""

from __future__ import annotations

import argparse
import collections
import ctypes
import ctypes.wintypes as wt
import json
import os
import subprocess
import sys
import time

KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
PSAPI = ctypes.WinDLL("psapi", use_last_error=True)

PROCESS_QUERY_INFORMATION = 0x0400
PROCESS_VM_READ = 0x0010
MEM_COMMIT = 0x1000
MEM_PRIVATE = 0x20000
MEM_MAPPED = 0x40000
MEM_IMAGE = 0x1000000
PAGE_GUARD = 0x100


class MEMORY_BASIC_INFORMATION64(ctypes.Structure):
    _fields_ = [
        ("BaseAddress", ctypes.c_ulonglong),
        ("AllocationBase", ctypes.c_ulonglong),
        ("AllocationProtect", wt.DWORD),
        ("__alignment1", wt.DWORD),
        ("RegionSize", ctypes.c_ulonglong),
        ("State", wt.DWORD),
        ("Protect", wt.DWORD),
        ("Type", wt.DWORD),
        ("__alignment2", wt.DWORD),
    ]


KERNEL32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
KERNEL32.OpenProcess.restype = wt.HANDLE
KERNEL32.VirtualQueryEx.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    ctypes.POINTER(MEMORY_BASIC_INFORMATION64),
    ctypes.c_size_t,
]
KERNEL32.VirtualQueryEx.restype = ctypes.c_size_t
KERNEL32.CloseHandle.argtypes = [wt.HANDLE]
PSAPI.GetMappedFileNameW.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    wt.LPWSTR,
    wt.DWORD,
]
PSAPI.GetMappedFileNameW.restype = wt.DWORD


def kind_label(mbi: MEMORY_BASIC_INFORMATION64) -> str:
    if mbi.Type == MEM_IMAGE:
        return "image"
    if mbi.Type == MEM_MAPPED:
        return "mapped"
    if mbi.Type == MEM_PRIVATE:
        return "private"
    return f"type-{mbi.Type:#x}"


def protection_label(protect: int) -> str:
    names = {
        0x01: "noaccess",
        0x02: "r",
        0x04: "rw",
        0x08: "w",
        0x10: "x",
        0x20: "rx",
        0x40: "rwx",
        0x80: "wc",
    }
    base = names.get(protect & 0xFF, f"{protect:#x}")
    if protect & PAGE_GUARD:
        base += "+guard"
    return base


def mapped_name(handle: int, address: int) -> str | None:
    buffer = ctypes.create_unicode_buffer(1024)
    length = PSAPI.GetMappedFileNameW(handle, ctypes.c_void_p(address), buffer, 1024)
    if not length:
        return None
    return buffer.value


def walk(pid: int) -> dict:
    handle = KERNEL32.OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, False, pid)
    if not handle:
        raise OSError(ctypes.get_last_error(), "OpenProcess failed (process gone?)")
    mbi = MEMORY_BASIC_INFORMATION64()
    address = 0
    limit = 1 << 47
    totals: collections.Counter = collections.Counter()
    by_file: collections.Counter = collections.Counter()
    private_blocks: list[tuple[int, int, str]] = []
    private_by_protect: collections.Counter = collections.Counter()
    regions = 0
    try:
        while address < limit:
            written = KERNEL32.VirtualQueryEx(
                handle, ctypes.c_void_p(address), ctypes.byref(mbi), ctypes.sizeof(mbi)
            )
            if written == 0:
                # End of the address space, or the process exited mid-walk.
                break
            regions += 1
            if mbi.State == MEM_COMMIT and mbi.RegionSize:
                kind = kind_label(mbi)
                totals[kind] += mbi.RegionSize
                if kind == "private":
                    private_by_protect[protection_label(mbi.Protect)] += mbi.RegionSize
                    private_blocks.append(
                        (mbi.RegionSize, mbi.BaseAddress, protection_label(mbi.Protect))
                    )
                else:
                    name = mapped_name(handle, mbi.BaseAddress) or "<anonymous>"
                    by_file[os.path.basename(name)] += mbi.RegionSize
            address = mbi.BaseAddress + mbi.RegionSize
    finally:
        KERNEL32.CloseHandle(handle)
    private_blocks.sort(reverse=True)
    return {
        "pid": pid,
        "regions": regions,
        "totals_mb": {kind: round(value / (1024 * 1024), 1) for kind, value in totals.items()},
        "private_by_protection_mb": {
            key: round(value / (1024 * 1024), 1) for key, value in private_by_protect.most_common()
        },
        "largest_private_mb": [
            {"mb": round(size / (1024 * 1024), 2), "at": f"{base:#x}", "protect": protect}
            for size, base, protect in private_blocks[:20]
        ],
        "files_mb": {
            name: round(value / (1024 * 1024), 1)
            for name, value in by_file.most_common(25)
        },
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Attribute a Windows process's memory.")
    parser.add_argument("--pid", type=int, default=None)
    parser.add_argument("--launch", type=str, default=None, help="command line to launch")
    parser.add_argument("--delay", type=float, default=6.0, help="seconds before walking")
    parser.add_argument("--json", type=str, default=None)
    args = parser.parse_args(argv)
    process = None
    if args.pid is None:
        if not args.launch:
            parser.error("pass --pid or --launch")
        process = subprocess.Popen(args.launch.split())
        time.sleep(args.delay)
        pid = process.pid
    else:
        pid = args.pid
    if process is not None and process.poll() is not None:
        raise SystemExit(f"the target exited with {process.returncode} before the walk")
    try:
        report = walk(pid)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            process.wait(timeout=20)
    text = json.dumps(report, indent=2)
    print(text)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
