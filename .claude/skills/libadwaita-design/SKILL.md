---
name: libadwaita-design
description: Use ao criar ou alterar a interface do MicBridge, seja o app GTK4/libadwaita (app/src/main.rs, widgets.rs, theme.rs, data/style.css) ou a página do celular (app/web). Aplica o DESIGN.md e as convenções do GNOME HIG, e ensina a capturar a janela para conferir o visual.
---

# Design da interface do MicBridge

Leia `DESIGN.md` na raiz antes de mexer em cores, espaçamentos ou componentes. Ele é a fonte
única; se uma decisão mudar, atualize-o no mesmo commit.

## Regras do app GTK4/libadwaita

- Prefira widgets e classes do libadwaita (`PreferencesGroup`, `ActionRow`, `ExpanderRow`,
  `SwitchRow`, `.card`, `.boxed-list`, `.pill`, `.suggested-action`, `.dim-label`) e refine
  com CSS em `app/data/style.css`. Não desenhe o que o libadwaita já oferece.
- Todo o visual customizado fica em `style.css`, com classes `mb-*`. Nada de cor fixa no Rust.
  Cores de status: use as classes `ok`, `warn`, `err`, `off` já definidas.
- Respeite tema claro e escuro: use as variáveis do libadwaita (`@window_bg_color`,
  `--accent-bg-color`) quando a cor não for do nosso sistema de status.
- Dialogs sempre com `adw::HeaderBar` e um botão de fechar visível, além do Esc.
- Textos em português do Brasil, curtos, sem jargão. Termos técnicos (RNNoise, gate) só em
  "Ajustes avançados".
- Animação só para estados vivos (conectando, erro, voz). Nunca decorativa.
- Antes de criar um widget novo, veja `app/src/widgets.rs`; reaproveite.
- **Classe CSS na janela: use `add_css_class`, nunca `.css_classes([...])` no construtor.** Essa
  propriedade apaga as classes `csd`/`background`, e a janela perde os cantos arredondados e a
  sombra. Pelo mesmo motivo, não pinte o nó `window` com `background`: ele cobre o retângulo
  inteiro e some o arredondamento; pinte o conteúdo (`toolbarview`).

## Dialeto de CSS do GTK (limites)

Sem `gap`, `backdrop-filter`, `text-transform`, `flex`/`grid`. Existem: `linear-gradient`,
`box-shadow`, `border-radius`, `transition`, `@keyframes`/`animation`, `letter-spacing`,
`min-width`/`min-height`, `opacity` e variáveis `var(--nome)`.

## Conferir o visual

O build de debug captura a janela e sai. Use outro id de aplicativo (para não esbarrar na
instância única do app instalado) e dados temporários (o QR de primeiro pareamento aparece e o
pareamento real não é tocado):

```sh
cd app && cargo build
XDG_DATA_HOME=/tmp/mbshot MICBRIDGE_APP_ID=dev.wilian.MicBridgeDev \
  MICBRIDGE_SNAPSHOT=/tmp/mb.png MICBRIDGE_SNAPSHOT_SIZE=560x1100 ./target/debug/micbridge
```

A captura às vezes falha na primeira tentativa; repita.

Abra `/tmp/mb.png` (ferramenta Read). Variáveis (só em build de debug):

- `MICBRIDGE_SNAPSHOT=<png>`: salva a captura 2 s depois de abrir e encerra.
- `MICBRIDGE_SNAPSHOT_SIZE=LARGURAxALTURA`: janela maior, para ver a página inteira.
- `MICBRIDGE_APP_ID=<id>`: roda ao lado do app instalado.
- `MICBRIDGE_NO_THEME=1`: libadwaita puro, para comparar (por exemplo, os cantos).
- `MICBRIDGE_DEMO=<estado>`: força o estado (`connected`, `voice`, `hot`, `warn`, `error`),
  fecha o pareamento e expande "Ajustes avançados".

Confira sempre: pareamento aberto (QR) e fechado, e os estados de status. Para os cantos, o
PNG deve ter alfa < 255 nos pixels de canto (o libadwaita puro dá `0,0,0,0`); alfa 255 em todos
os cantos significa janela quadrada. O app é só no
tema escuro (`ForceDark`); não é preciso validar o claro.

## Página do celular

HTML, CSS e JS puros em `app/web`, embutidos no binário (`include_str!`). Mesmos tokens do
`DESIGN.md`. Alvo de toque ≥ 48 px, `prefers-reduced-motion` respeitado, CSP restrita (sem
scripts ou estilos inline).
