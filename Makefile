# =============================================================================
#  corne-rs — comandos do projeto.   `make` (ou `make help`) lista tudo.
# =============================================================================

# Complementos incluídos no firmware (vírgula, sem espaços). Ex.: make build FEATURES=rgb
FEATURES ?= rgb,oled

# Opções do preview (todas opcionais):
#   EFFECT=aurora|ripple|3...   THEME=0..4   LEFT=status   RIGHT=stars
#   LAYER=1   CAPS=1   LANDSCAPE=1   NOTYPING=1   FRAMES=300
EFFECT    ?=
THEME     ?=
LEFT      ?=
RIGHT     ?=
LAYER     ?=
CAPS      ?=
LANDSCAPE ?=
NOTYPING  ?=
FRAMES    ?=

FW_DIR  := firmware
OUT     := out
ELF     := $(FW_DIR)/target/thumbv6m-none-eabi/release/corne-fw
PREVIEW := target/release/corne-preview
EFFECTS := solid gradient breath rainbow aurora plasma ripple heat stars test
SCENES  := status wpm stars rain life plasma keys

PREVIEW_ARGS := $(if $(EFFECT),--effect $(EFFECT)) \
                $(if $(THEME),--theme $(THEME)) \
                $(if $(LEFT),--scene-left $(LEFT)) \
                $(if $(RIGHT),--scene-right $(RIGHT)) \
                $(if $(LAYER),--layer $(LAYER)) \
                $(if $(CAPS),--caps) \
                $(if $(LANDSCAPE),--landscape) \
                $(if $(NOTYPING),--no-typing) \
                $(if $(FRAMES),--frames $(FRAMES))

# Os dois lados compilam no mesmo diretório: nada de build em paralelo.
.NOTPARALLEL:
.DEFAULT_GOAL := help

.PHONY: help setup test check clippy fmt fmt-check keymap ci \
        preview preview-list preview-tour preview-scenes preview-themes preview-leds preview-layers \
        build left right build-rgb build-oled build-bare size \
        flash-left flash-right clean

## ---------- Geral ----------

help: ## Mostra esta ajuda
	@awk 'BEGIN {FS = ":.*## "} \
	  /^## / { printf "\n\033[1m%s\033[0m\n", substr($$0, 4); next } \
	  /^[a-zA-Z_-]+:.*## / { printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo
	@echo "Variáveis: FEATURES=$(FEATURES)  EFFECT THEME LEFT RIGHT LAYER CAPS LANDSCAPE NOTYPING FRAMES"
	@echo "Exemplos:  make preview EFFECT=ripple THEME=4 RIGHT=rain"
	@echo "           make build FEATURES=rgb      make flash-left"

setup: ## Instala o alvo do RP2040 e as ferramentas do Rust
	rustup target add thumbv6m-none-eabi
	rustup component add llvm-tools rustfmt clippy

## ---------- Qualidade ----------

test: ## Roda os testes do core, RGB e OLED (no PC)
	cargo test

check: ## Verifica se tudo compila (PC e firmware dos dois lados)
	cargo check --workspace
	cd $(FW_DIR) && cargo check --release --no-default-features --features left,$(FEATURES)
	cd $(FW_DIR) && cargo check --release --no-default-features --features right,$(FEATURES)

clippy: ## Lints do clippy em tudo
	cargo clippy --workspace
	cd $(FW_DIR) && cargo clippy --release --features left

fmt: ## Formata o código
	cargo fmt --all
	cd $(FW_DIR) && cargo fmt

fmt-check: ## Confere a formatação sem alterar nada
	cargo fmt --all -- --check
	cd $(FW_DIR) && cargo fmt -- --check

keymap: $(PREVIEW) ## Valida keymap.kbd, combos.kbd e macros.kbd, desenha as camadas e lista combos e macros
	@$(PREVIEW) --keymap

ci: fmt-check test check ## Tudo que precisa passar antes de gravar

## ---------- Preview no terminal ----------

$(PREVIEW): $(shell find crates -name '*.rs' -o -name Cargo.toml) keymap.kbd combos.kbd macros.kbd
	cargo build --release -p corne-preview

preview: $(PREVIEW) ## Preview de RGB + OLED (Ctrl+C sai). Veja as variáveis acima
	@$(PREVIEW) $(PREVIEW_ARGS)

preview-list: $(PREVIEW) ## Lista efeitos, temas e cenas
	@$(PREVIEW) --list

preview-tour: $(PREVIEW) ## Passa por todos os efeitos de RGB (~4 s cada)
	@for e in $(EFFECTS); do \
	  $(PREVIEW) --effect $$e $(if $(THEME),--theme $(THEME)) --frames 120 || exit 1; \
	done

preview-scenes: $(PREVIEW) ## Passa por todas as cenas do OLED (~4 s cada)
	@for s in $(SCENES); do \
	  $(PREVIEW) --scene-left $$s --scene-right $$s $(if $(LANDSCAPE),--landscape) --frames 120 || exit 1; \
	done

preview-themes: $(PREVIEW) ## Troca de tema a cada 3 s: mostra o aviso no OLED direito
	@$(PREVIEW) --theme-cycle $(if $(EFFECT),--effect $(EFFECT)) $(if $(LANDSCAPE),--landscape) $(if $(FRAMES),--frames $(FRAMES))

preview-leds: $(PREVIEW) ## Efeito Test: acende os LEDs na ordem da corrente
	@$(PREVIEW) --effect test --no-typing $(if $(FRAMES),--frames $(FRAMES))

preview-layers: $(PREVIEW) ## Mostra o destaque de cada camada (~4 s cada)
	@n=$$($(PREVIEW) --keymap | sed -n 's/^keymap.kbd OK: \([0-9]*\).*/\1/p'); \
	for l in $$(seq 0 $$((n - 1))); do \
	  $(PREVIEW) --layer $$l --frames 120 || exit 1; \
	done

## ---------- Firmware ----------

build: test left right ## Testa e gera out/corne-left.uf2 e out/corne-right.uf2
	@echo
	@echo "Pronto: $(OUT)/corne-left.uf2  $(OUT)/corne-right.uf2"
	@command -v wslpath >/dev/null && printf "No Windows: %s\n" "$$(wslpath -w $(CURDIR)/$(OUT))" || true

left: ## Gera só out/corne-left.uf2
	@$(MAKE) --no-print-directory uf2 SIDE=left

right: ## Gera só out/corne-right.uf2
	@$(MAKE) --no-print-directory uf2 SIDE=right

build-rgb: ## Firmware só com RGB (sem OLED)
	@$(MAKE) --no-print-directory build FEATURES=rgb

build-oled: ## Firmware só com OLED (sem RGB)
	@$(MAKE) --no-print-directory build FEATURES=oled

build-bare: ## Firmware sem complementos (só o core)
	@$(MAKE) --no-print-directory build FEATURES=

# Interno: compila um lado e converte para UF2.
.PHONY: uf2
uf2:
	@test -n "$(SIDE)" || { echo "use: make left | make right"; exit 1; }
	@mkdir -p $(OUT)
	@echo "==> $(SIDE) ($(SIDE)$(if $(FEATURES),$(comma)$(FEATURES)))"
	cd $(FW_DIR) && cargo build --release --no-default-features --features "$(SIDE)$(if $(FEATURES),$(comma)$(FEATURES))"
	@python3 -I scripts/elf2uf2.py $(ELF) $(OUT)/corne-$(SIDE).uf2

comma := ,

size: ## Mostra o uso de flash e RAM de cada lado
	@SZ=$$(find $$(rustc --print sysroot) -name llvm-size -type f | head -1); \
	test -n "$$SZ" || { echo "rode: make setup"; exit 1; }; \
	for side in left right; do \
	  (cd $(FW_DIR) && cargo build -q --release --no-default-features --features "$$side$(if $(FEATURES),$(comma)$(FEATURES))") || exit 1; \
	  $$SZ -A $(ELF) | awk -v s=$$side ' \
	    /^\.(text|rodata|boot2|data)/ { flash += $$2 } \
	    /^\.(data|bss|uninit)/ { ram += $$2 } \
	    END { printf "%-6s flash %6.1f KB / 1984 KB   RAM %5.1f KB / 256 KB\n", s, flash/1024, ram/1024 }'; \
	done

## ---------- Gravar ----------

flash-left: left ## Gera, grava e reinicia a metade esquerda (sem apertar nada)
	@scripts/flash.sh $(OUT)/corne-left.uf2 left

flash-right: right ## Gera, grava e reinicia a metade direita (sem apertar nada)
	@scripts/flash.sh $(OUT)/corne-right.uf2 right

## ---------- Limpeza ----------

clean: ## Apaga builds e .uf2
	cargo clean
	cd $(FW_DIR) && cargo clean
	rm -rf $(OUT)
