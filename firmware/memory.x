/* RP2040 com flash de 2 MB (Pro Micro RP2040).
 * Os últimos 64 KB da flash ficam reservados: o primeiro setor (4 KB) guarda
 * as configurações de RGB/OLED (src/persist.rs); o resto fica para o keymap. */
MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100 - 64K
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}
