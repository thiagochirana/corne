# corne-rs

Firmware em Rust para o **Corne V3 3x6+3** com **Pro Micro RP2040**, OLED em
cada metade, RGB por tecla e cabo **TRS** entre as metades. Feito do zero
sobre o [Embassy](https://embassy.dev), sem QMK.

## Estrutura

```
keymap.kbd              ← SEU LAYOUT (texto, editável)
combos.kbd              ← SEUS COMBOS (várias teclas juntas => uma ação)
macros.kbd              ← SUAS MACROS (textos e sequências de teclas)
secrets.env             ← SEUS SEGREDOS para ENV(NOME) nas macros (fora do git)
secrets.env.example     modelo do secrets.env (vai para o git, sem valores)
crates/
  corne-core/           núcleo: keycodes, camadas, keymap, tap-hold,
                        protocolo split e protocolo de configuração (sem hardware)
  corne-rgb/            complemento RGB: efeitos, temas por lado, sobreposições
  corne-oled/           complemento OLED: cenas animadas, framebuffer
  corne-preview/        mostra RGB e OLED no terminal, sem gravar o teclado
firmware/               liga tudo ao RP2040 (matriz, USB, link TRS, LEDs, OLED)
  src/board.rs          ← pinos (único lugar para mudar)
  src/user_config.rs    ← cenas do OLED por lado, rotação, tap-hold, brilho inicial
Makefile                todos os comandos (make help)
scripts/                flash.sh, corne-hid.ps1/corne_hid.py, elf2uf2.py (usados pelo Makefile); build.sh
```

O **core não depende dos complementos**. RGB e OLED só leem o estado do
teclado e os eventos de tecla. Para tirar um deles, use as features do Cargo:
`make build FEATURES=rgb` gera um firmware só com RGB.

## Uso rápido

Tudo passa pelo `make` (`make` sozinho lista os comandos):

```bash
make setup                 # uma vez: alvo do RP2040 + ferramentas
make keymap                # valida o keymap.kbd e desenha as camadas
make preview               # RGB + OLED no terminal (Ctrl+C sai)
make preview EFFECT=ripple THEME=4 RIGHT=rain LAYER=1
make preview-tour          # todos os efeitos de RGB
make preview-scenes        # todas as cenas do OLED
make preview-themes        # troca de tema e mostra o aviso no OLED direito
make test                  # testes no PC
make ci                    # formatação + testes + compila os dois lados
make build                 # gera out/corne-left.uf2 e out/corne-right.uf2
make build FEATURES=rgb    # só com RGB (ou build-rgb, build-oled, build-bare)
make size                  # uso de flash e RAM
make flash-left            # gera, grava e reinicia (sem apertar nada)
```

### Gravar

1. Ligue a metade na USB.
2. Rode `make flash-left` (ou `flash-right`). O script:
   - pergunta ao teclado qual metade está na USB e recusa se for o lado
     errado (`FORCE=1` pula essa conferência);
   - manda o comando de bootloader, e a metade vira a unidade `RPI-RP2`;
   - copia o `.uf2`, e o RP2040 grava e reinicia sozinho;
   - espera o teclado voltar pela USB e confirma.
3. Para a outra metade, passe o cabo USB para ela e rode o outro comando.

Se o teclado não responder (primeira gravação ou outro firmware), o script
pede o **duplo reset** e continua sozinho. No WSL2 tudo passa pelo Windows
(PowerShell, `scripts/corne-hid.ps1`), sem `usbipd`. Para forçar a letra da
unidade: `DRIVE=E`. No Linux nativo usa `scripts/corne_hid.py` (`/dev/hidraw*`).

O duplo toque no botão de reset da PCB é tratado por este firmware (janela de
500 ms, `DOUBLE_TAP_MS` em `user_config.rs`). **Na primeira gravação**, quem
responde ao botão ainda é o firmware antigo: se o duplo toque não abrir a
unidade `RPI-RP2`, segure o botão **BOOT** do Pro Micro RP2040 e aperte o
reset (ou ligue o cabo USB com o BOOT apertado). Depois disso, o duplo toque
passa a funcionar sempre.

Também dá para copiar manualmente `out/corne-*.uf2` para a unidade; o
`make build` mostra o caminho `\\wsl.localhost\...` para abrir no Explorer.

A tecla `BOOT` (camada `adjust`) também entra no modo de gravação, mas reinicia
**a metade ligada na USB**. Para a outra, use o duplo reset.

## Layout (`keymap.kbd`)

Cada camada é um bloco `[nome]` com 42 teclas (3 linhas de 12 + 6 polegares).
O arquivo tem a referência de todas as ações no topo. Exemplos:

```
LT(nav, SPC)     toque = espaço, segura = camada nav
SHIFT_T(A)       toque = A, segura = Shift (home row mod)
CTRL(SHIFT(TAB)) Ctrl+Shift+Tab
MO(sym) TG(2)    camada enquanto segura / liga-desliga
```

Erros aparecem na compilação com linha e motivo, antes de ir para o teclado.

### Tap-hold sem engasgo (`LT(sym, SPC)` etc.)

Configurado em `ENGINE`, no `firmware/src/user_config.rs`:

- **Flow tap (`flow_tap_ms = 150`):** logo depois de uma letra, número ou
  pontuação, um `LT`/`MT` vale como toque na hora. O espaço depois de uma
  palavra nunca vira camada e sai sem atraso. Para usar a camada logo depois de
  digitar, espere um instante (150 ms).
- **Permissive hold:** depois de uma pausa, a camada só liga se outra tecla for
  apertada **e solta** com o `LT` segurado. Teclas sobrepostas (`SPC↓ E↓ SPC↑
  E↑`) continuam sendo espaço + e.
- **Retro tap:** segurar o espaço e soltar sem apertar nada ainda digita o espaço.
- Se você soltar uma letra enquanto o `LT` decide, ela sobe na hora.

### Combos (`combos.kbd`)

Duas ou mais teclas apertadas juntas disparam uma ação, qualquer uma que o
keymap aceite (inclusive `USER(n)`, onde vão entrar as macros):

```
term = 40            // janela em ms para apertar todas
J K      => ESC      // vale em todas as camadas
S D F    => CTRL(Z)  // quantas teclas quiser
[num]                // daqui para baixo: só na camada num
LEFT DOWN => CTRL(Z)
```

As teclas são escritas como aparecem no keymap (ou `K(linha, coluna)`). Só as
teclas que fazem parte de algum combo esperam a janela; as outras não têm
atraso. `make keymap` valida e lista os combos.

`idle = 150` (padrão): no meio de uma palavra nenhum combo começa, então "po"
de "tempo" sai como texto mesmo com `p o` sendo um combo. Faça uma pausa curta
antes de um combo; `idle = 0` desliga.

### Macros (`macros.kbd`)

Uma macro é uma sequência de teclas tocada por uma tecla (`MACRO(nome)` no
`keymap.kbd`) ou por um combo (`J K L => MACRO(nome)` no `combos.kbd`):

```
delay = 0                         // ms entre teclas (aumente se um programa perder letras)

[assinatura]
"Atenciosamente," ENT "Seu Nome"  // texto com acentos e ç (usa o host)

[print]
DOWN(WIN) DOWN(SHIFT) S UP(SHIFT) UP(WIN)

[copia_linha]
HOME S(END) WAIT(20) C(C)
```

Passos: `"texto"`, teclas e atalhos (`ESC`, `CTRL(Z)`), `DOWN(tecla)`/`UP(tecla)`
e `WAIT(ms)`. Teclas apertadas enquanto a macro toca esperam e saem depois, na
ordem. `make keymap` valida as macros e mostra onde cada uma é usada.

### Senhas e segredos (`secrets.env`)

Para não escrever senhas no `macros.kbd`, use `ENV(NOME)` e coloque o valor no
`secrets.env`, que fica **fora do git** (`.gitignore`):

```
# secrets.env                      # macros.kbd                # combos.kbd
SENHA_PC=minha senha               [senha_pc]                  p j => MACRO(senha_pc)
                                   ENV(SENHA_PC)
```

- Modelo para copiar: `cp secrets.env.example secrets.env`.
- Também vale uma variável de ambiente com o mesmo nome (o `secrets.env` tem prioridade).
- Se faltar um valor, o build para com erro, e as mensagens nunca mostram o valor.
  O `make keymap` mostra essas macros como "secreta", sem o tamanho.

**Limites:** isso protege o **repositório**. O valor continua dentro do firmware
(`out/*.uf2`, `firmware/target/`), e o RP2040 não protege a flash contra
leitura: quem tiver o teclado na mão consegue copiar a memória, e quem apertar o
combo digita a senha. Use para segredos de baixo risco, deixe o combo numa
camada específica e não termine a macro com `ENT`.

### Cenas do OLED

Cada lado tem sua lista de cenas (`LEFT_SCENES` e `RIGHT_SCENES` em
`firmware/src/user_config.rs`) e sua própria posição nela:

| Tecla | Troca |
|---|---|
| `OLED_NEXT` / `OLED_PREV` | os dois lados |
| `OLED_NEXT_RIGHT` / `OLED_PREV_RIGHT` | só o direito (mostra o nome da cena por 2 s) |
| `OLED_NEXT_LEFT` / `OLED_PREV_LEFT` | só o esquerdo |

Cenas: `Status`, `WPM`, `Keys` (informação) e as animadas `Stars`, `Rain`,
`Life`, `Plasma`, `Fire` (fogo que cresce ao digitar), `Burst` (cada tecla lança
um fogo de artifício), `EQ` (equalizador que pula com as teclas), `Pong` (joga
sozinho, acelera com o WPM), `Warp` (túnel 3D), `Blobs` (lava), `DNA` (hélice
girando) e `Scope` (osciloscópio). Para ver no terminal: `make preview-scenes`
(todas) ou `make preview-right` (troca só a direita, como `OLED_NEXT_RIGHT`).

### Configurações salvas (RGB e OLED)

O que as teclas `RGB_*` e `OLED_*` mudam (efeito, tema, cor, brilho,
velocidade, liga/desliga e a cena de cada OLED) fica gravado na flash e volta
igual quando você religa o teclado. A gravação acontece 3 s depois da última
mudança, nas duas metades, então tanto faz qual lado vai na USB.

- Regravar o firmware (`.uf2`) **não** apaga o que foi salvo.
- Se você mudar os valores iniciais (`SETTINGS` em `firmware/src/user_config.rs`)
  e regravar, o que estava salvo é descartado e os valores novos passam a valer.
- Para desligar: `SAVE_SETTINGS = false`. A espera fica em `SAVE_DELAY_MS`.

### Acentos e ç

O teclado manda posições de tecla e quem escolhe o caractere é o sistema. Por
isso o `keymap.kbd` diz qual layout o Windows/Linux usa:

```
host = abnt2     // ou us-intl, ou us
```

Com isso, escreva o caractere que quer (`` ç Ç ´ ` ~ ^ ¨ ``, ou `CCED ACUT GRAVE
TILD CIRC DIAE`) e o firmware escolhe a tecla certa. Os símbolos (`[ ] / ; \ '`)
também são traduzidos. Acentos são teclas mortas: `´` e depois `a` = `á`.

## Camadas configuráveis em tempo real (estilo Vial)

A base já está pronta no core:

- Toda ação tem uma codificação `u16` estável (`corne_core::action`).
- O keymap fica em RAM e pode ser editado tecla a tecla (`Keymap::set`).
- Há um protocolo de 32 bytes por **raw HID** (usage page `0xFF60`) com
  `GetInfo`, `GetKey`, `SetKey`, `GetRow`, `ResetKeymap`, `Get/SetSettings`
  e `Bootloader` (`corne_core::protocol`), já ligado na USB do firmware.
- O parser do `keymap.kbd` é `no_std`, então dá para reaproveitá-lo num configurador.

Ainda falta:

1. Gravar o keymap na flash. As configurações de RGB/OLED já são salvas
   (veja acima, e o comando `Save` do protocolo grava na hora); as teclas
   alteradas pelo protocolo ainda se perdem ao reiniciar.
2. Um configurador no PC (CLI ou página web com WebHID) que use o protocolo.

## Pontos para conferir no hardware

O código compila e os testes passam, mas **ainda não rodou num teclado de verdade**.
Itens que dependem da sua placa:

| Item | Onde ajustar |
|---|---|
| Pinagem do controlador (assumida: RP2040 "Community Edition", a mesma do `rp2040_ce` do QMK) | `firmware/src/board.rs` |
| Ordem dos LEDs na corrente (assumida: a do QMK para o `crkbd`) | `crates/corne-rgb/src/leds.rs` (o efeito `Test` acende um por um) |
| Placa sem underglow | apague as 6 entradas `Under` em `leds.rs` |
| Aviso de tema no OLED (lado direito, 2 s) | `oled::POPUPS` em `user_config.rs` |
| OLED de cabeça para baixo | `ORIENTATION` em `firmware/src/user_config.rs` |
| Placa não inicia depois de gravar | troque `boot2-w25q080` por `boot2-generic-03h` em `firmware/Cargo.toml` |
| Erros no link com cabo longo (contador `ERR`/`E` na cena Status do OLED central subindo rápido) | `SPLIT_BAUD` em `user_config.rs` |
| Toques fantasmas (tecla saindo sozinha) | `DEBOUNCE_MODE = DebounceMode::Defer` em `user_config.rs` |

O brilho total é limitado a ~25% (`POWER_LIMIT` em `corne-rgb`) para não
passar dos 500 mA da USB.
