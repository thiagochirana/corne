#!/usr/bin/env bash
# Grava um .uf2 numa metade do teclado, sem precisar apertar nada.
#
# Uso: scripts/flash.sh arquivo.uf2 [left|right]
#
# 1. Pergunta ao teclado (raw HID) qual metade está na USB e confere se é o
#    lado deste .uf2 (FORCE=1 pula a conferência).
# 2. Manda o comando de bootloader: a metade reinicia como unidade RPI-RP2.
# 3. Copia o .uf2; o RP2040 grava e reinicia sozinho no firmware novo.
# 4. Espera o teclado voltar pela USB e confirma.
#
# Se o teclado não responder (primeira gravação, outro firmware), cai no modo
# manual: aperte o reset duas vezes.
#
# - WSL2: fala com o teclado e com a unidade pelo Windows (PowerShell).
# - Linux: usa /dev/hidraw* (scripts/corne_hid.py) e /media/$USER/RPI-RP2.
# - DRIVE=E força a letra da unidade no WSL.
set -euo pipefail

UF2="${1:?uso: flash.sh arquivo.uf2 [left|right]}"
WANT="${2:-}"
DIR="$(cd "$(dirname "$0")" && pwd)"
[ -f "$UF2" ] || { echo "arquivo não encontrado: $UF2" >&2; exit 1; }

is_wsl() { grep -qi microsoft /proc/version 2>/dev/null; }

# Comando para o teclado: info | boot. Imprime a resposta (left, right, none...).
hid() {
  if is_wsl; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$DIR/corne-hid.ps1")" "$1" \
      2>/dev/null | tr -d '\r' | head -n1
  else
    python3 "$DIR/corne_hid.py" "$1" 2>/dev/null | head -n1
  fi
}

# Destino da cópia (letra no Windows ou ponto de montagem no Linux), se a unidade existir.
find_target() {
  if is_wsl; then
    if [ -n "${DRIVE:-}" ]; then
      echo "${DRIVE%%:*}"
      return
    fi
    powershell.exe -NoProfile -Command \
      "(Get-Volume -FileSystemLabel 'RPI-RP2' -ErrorAction SilentlyContinue).DriveLetter" \
      2>/dev/null | tr -d '\r\n '
  else
    for d in "/media/${USER:-}/RPI-RP2" "/run/media/${USER:-}/RPI-RP2" /mnt/RPI-RP2; do
      if [ -f "$d/INFO_UF2.TXT" ]; then
        echo "$d"
        return
      fi
    done
  fi
}

# Espera a unidade por até $1 segundos; deixa o destino em $TARGET.
wait_drive() {
  for _ in $(seq "$1"); do
    TARGET="$(find_target || true)"
    [ -n "$TARGET" ] && return 0
    sleep 1
  done
  return 1
}

copy_uf2() {
  if is_wsl; then
    local src
    src="$(wslpath -w "$(realpath "$UF2")")"
    echo "Gravando $(basename "$UF2") em ${TARGET}:\\ ..."
    # O RP2040 reinicia no último bloco; o Windows às vezes reclama disso.
    powershell.exe -NoProfile -Command "Copy-Item -LiteralPath '$src' -Destination '${TARGET}:\\'" \
      >/dev/null 2>&1 || true
  else
    echo "Gravando $(basename "$UF2") em $TARGET ..."
    cp "$UF2" "$TARGET/"
    sync
  fi
}

# Espera o teclado voltar pela USB rodando o firmware novo.
wait_back() {
  printf "Reiniciando"
  for _ in $(seq 20); do
    sleep 1
    printf "."
    local side
    side="$(hid info || true)"
    if [ "$side" = left ] || [ "$side" = right ]; then
      echo
      echo "Pronto: firmware novo rodando (metade $side na USB)."
      return 0
    fi
  done
  echo
  echo "Gravado, mas o teclado não voltou pela USB em 20 s. Confira o cabo." >&2
  return 1
}

manual() {
  echo "Aperte o reset da metade duas vezes rápido..."
  if ! wait_drive 60; then
    echo "A unidade RPI-RP2 não apareceu em 60 s." >&2
    echo "Copie manualmente: $UF2" >&2
    exit 1
  fi
}

# Já está no modo de gravação?
TARGET="$(find_target || true)"
if [ -z "$TARGET" ]; then
  side="$(hid info || true)"
  case "$side" in
    left | right)
      if [ -n "$WANT" ] && [ "$side" != "$WANT" ] && [ "${FORCE:-}" != 1 ]; then
        echo "A metade ligada na USB é a '$side', mas este firmware é da '$WANT'." >&2
        echo "Ligue a metade '$WANT' na USB, ou use: make flash-$side" >&2
        exit 1
      fi
      echo "Metade '$side' encontrada. Entrando no modo de gravação..."
      hid boot >/dev/null || true
      if ! wait_drive 15; then
        echo "A unidade RPI-RP2 não apareceu sozinha."
        manual
      fi
      ;;
    *)
      echo "Teclado não encontrado pela USB (primeira gravação ou outro firmware)."
      manual
      ;;
  esac
fi

copy_uf2
wait_back
