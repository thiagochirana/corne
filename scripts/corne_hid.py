#!/usr/bin/env python3
"""Fala com o teclado pela interface raw HID (usage page 0xFF60), no Linux.

Usado pelo scripts/flash.sh fora do WSL. Mesma interface do corne-hid.ps1:

    corne_hid.py info   -> imprime "left", "right", "unknown" ou "none"
    corne_hid.py boot   -> reinicia a metade da USB no modo de gravação

Códigos de saída: 0 ok, 2 teclado não encontrado, 3 sem resposta, 4 erro de E/S.
Sem permissão em /dev/hidraw*? Crie uma regra udev, ex.:
    KERNEL=="hidraw*", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="0001", MODE="0666"
"""

import glob
import os
import select
import sys

HID_ID = "0003:00001209:00000001"


def find():
    for d in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            with open(d + "/device/uevent") as f:
                if "HID_ID=" + HID_ID not in f.read().upper():
                    continue
            with open(d + "/device/report_descriptor", "rb") as f:
                if f.read(3) == b"\x06\x60\xff":
                    return "/dev/" + os.path.basename(d)
        except OSError:
            pass
    return None


def serial(dev):
    # hidrawN -> dispositivo HID -> interface USB -> dispositivo USB (tem o serial)
    base = os.path.realpath("/sys/class/hidraw/" + os.path.basename(dev) + "/device")
    try:
        with open(os.path.join(base, "..", "..", "serial")) as f:
            return f.read().strip().lower()
    except OSError:
        return ""


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd not in ("info", "boot"):
        print(__doc__)
        return 1
    dev = find()
    if dev is None:
        print("none")
        return 2
    try:
        fd = os.open(dev, os.O_RDWR)
    except OSError as e:
        print(f"error: {e}")
        return 4
    try:
        # [report id 0, comando, ...] (o kernel tira o report id)
        os.write(fd, bytes([0, 0x09 if cmd == "boot" else 0x01]) + bytes(31))
        if cmd == "boot":
            print("ok")
            return 0
        r, _, _ = select.select([fd], [], [], 1.5)
        if not r:
            print("timeout")
            return 3
        res = os.read(fd, 64)
        side = "unknown"
        if len(res) >= 7 and res[0] == 0x01 and res[1] >= 2 and res[6] <= 1:
            side = "left" if res[6] == 0 else "right"
        else:
            # Firmware com protocolo antigo: o lado vem do número de série USB.
            sn = serial(dev)
            if sn in ("left", "right"):
                side = sn
        print(side)
        return 0
    except OSError as e:
        print(f"error: {e}")
        return 4
    finally:
        os.close(fd)


if __name__ == "__main__":
    sys.exit(main())
