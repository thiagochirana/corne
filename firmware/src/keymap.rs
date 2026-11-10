//! Keymap padrão, macros e combos, gerados de `../keymap.kbd`, `../macros.kbd`
//! e `../combos.kbd` pelo `build.rs`.

include!(concat!(env!("OUT_DIR"), "/keymap_gen.rs"));

/// Teclas "vivas" de cada camada (usado pelo RGB para destacar a camada ativa).
#[allow(dead_code)]
pub static LAYER_MASKS: [u64; LAYER_COUNT] = {
    let mut out = [0u64; LAYER_COUNT];
    let mut l = 0;
    while l < LAYER_COUNT {
        let mut i = 0;
        while i < corne_core::layout::KEY_COUNT {
            let pos = corne_core::layout::KeyPos::from_index(i);
            let v = DEFAULT_LAYERS[l][pos.row as usize][pos.col as usize];
            // 0x0000 = XXX, 0x0001 = ___
            if v > 1 {
                out[l] |= 1 << i;
            }
            i += 1;
        }
        l += 1;
    }
    out
};
