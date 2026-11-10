#!/usr/bin/env python3
"""Converte um ELF do RP2040 em .uf2 (sem dependências).

Uso: elf2uf2.py entrada.elf saida.uf2
"""
import struct
import sys

UF2_MAGIC_START0 = 0x0A324655
UF2_MAGIC_START1 = 0x9E5D5157
UF2_MAGIC_END = 0x0AB16F30
FLAG_FAMILY_ID = 0x00002000
RP2040_FAMILY_ID = 0xE48BFF56
FLASH_START = 0x10000000
FLASH_END = 0x10000000 + 16 * 1024 * 1024
PAGE = 256


def load_segments(path):
    data = open(path, "rb").read()
    if data[:4] != b"\x7fELF" or data[4] != 1:
        sys.exit("não é um ELF de 32 bits")
    (phoff,) = struct.unpack_from("<I", data, 0x1C)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x2A)
    segs = []
    for i in range(phnum):
        p_type, p_offset, _vaddr, p_paddr, p_filesz, _memsz, _flags, _align = struct.unpack_from(
            "<IIIIIIII", data, phoff + i * phentsize
        )
        if p_type != 1 or p_filesz == 0:  # PT_LOAD com conteúdo
            continue
        if not (FLASH_START <= p_paddr < FLASH_END):
            continue
        segs.append((p_paddr, data[p_offset : p_offset + p_filesz]))
    if not segs:
        sys.exit("nenhum segmento na flash")
    return segs


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    segs = load_segments(sys.argv[1])

    # Junta tudo numa imagem por páginas de 256 bytes.
    pages = {}
    for addr, blob in segs:
        for i, b in enumerate(blob):
            a = addr + i
            page = a & ~(PAGE - 1)
            pages.setdefault(page, bytearray(PAGE))[a - page] = b

    addrs = sorted(pages)
    out = bytearray()
    for n, addr in enumerate(addrs):
        header = struct.pack(
            "<IIIIIIII",
            UF2_MAGIC_START0,
            UF2_MAGIC_START1,
            FLAG_FAMILY_ID,
            addr,
            PAGE,
            n,
            len(addrs),
            RP2040_FAMILY_ID,
        )
        block = header + bytes(pages[addr]) + bytes(476 - PAGE) + struct.pack("<I", UF2_MAGIC_END)
        assert len(block) == 512
        out += block
    open(sys.argv[2], "wb").write(out)
    print(f"{sys.argv[2]}: {len(addrs)} blocos ({len(addrs) * PAGE // 1024} KB)")


if __name__ == "__main__":
    main()
