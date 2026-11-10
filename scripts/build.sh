#!/usr/bin/env bash
# Compila o firmware das duas metades e gera out/corne-left.uf2 e out/corne-right.uf2.
#
# Uso:  scripts/build.sh            (com RGB e OLED)
#       FEATURES=rgb scripts/build.sh   (só RGB; FEATURES="" para nenhum complemento)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
mkdir -p "$OUT"

FEATURES="${FEATURES-rgb,oled}"

# Testes do core e dos complementos antes de gerar firmware.
(cd "$ROOT" && cargo test --quiet -p corne-core)

for side in left right; do
  feats="$side${FEATURES:+,$FEATURES}"
  echo "==> $side ($feats)"
  (cd "$ROOT/firmware" && cargo build --release --no-default-features --features "$feats")
  python3 -I "$ROOT/scripts/elf2uf2.py" \
    "$ROOT/firmware/target/thumbv6m-none-eabi/release/corne-fw" \
    "$OUT/corne-$side.uf2"
done

echo
echo "Pronto. Para gravar cada metade:"
echo "  1. Ligue SÓ essa metade na USB (sem o cabo TRS)."
echo "  2. Aperte o reset duas vezes rápido: aparece a unidade RPI-RP2."
echo "  3. Copie o .uf2 correspondente para a unidade."
if command -v wslpath >/dev/null 2>&1; then
  echo
  echo "No Windows, os arquivos estão em: $(wslpath -w "$OUT")"
fi
