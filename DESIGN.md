# MicBridge: sistema de design

Fonte única das decisões visuais do app GTK4/libadwaita, da bandeja e da página do
celular. Ao mudar uma cor ou medida, atualize este arquivo junto.

## Princípios

1. **Calmo e escuro.** Superfícies grafite, bordas finas, brilho só onde há
   significado (status e voz). O app usa sempre o tema escuro (`ColorScheme::ForceDark`): o
   vidro e os brilhos foram desenhados para ele e não funcionam bem no claro.
2. **Status em destaque.** A primeira coisa na janela é o estado da conexão, grande e com cor.
3. **Perfis e controle direto.** Um perfil de voz resolve; módulos de áudio visíveis permitem refinar o som.
4. **Movimento com propósito.** Só anima o que está vivo (conectando, erro, voz). Sem enfeite.
5. **Nativo.** Usa widgets e classes do libadwaita; o CSS refina, não reinventa.

## Cores

| Token | Valor | Uso |
|-------|-------|-----|
| `bg-top` / `bg-bottom` | `#14161b` / `#14161b` | Fundo uniforme da janela |
| `surface` | `#20232a` | Cards e listas |
| `surface-border` | `rgba(255,255,255,.08)` | Borda de 1 px dos cards |
| `accent` | `#8aa2ff` (fundo `#5b78ee`) | Seleção, switches, botão principal. Substitui o acento do sistema em todas as janelas, exceto em botões `destructive-action`, `error`, `success` e `warning` |
| `text-dim` | alfa .62 do texto | Descrições e rótulos secundários |

### Status (padrão Discord, igual ao ícone da bandeja)

| Estado | Cor | Comportamento |
|--------|-----|---------------|
| Conectado | `#23A55A` | Fixo; anel pulsante quando há voz |
| Conectando / Reconectando | `#F0B232` | Pisca |
| Erro | `#F23F43` | Pisca |
| Desconectado | `#80848E` | Fixo |

Medidores: verde até 70 %, amarelo até 90 %, vermelho acima (proximidade de estourar).
A bolinha verde pulsa (anel) enquanto há voz chegando.

## Tipografia

Fonte do sistema (Adwaita Sans/Cantarell). Título do status 22 px / peso 700; rótulos de seção
11 px, peso 700, espaçamento de letras +0,08 em, cor `text-dim`; corpo no tamanho padrão do
libadwaita. Textos em português do Brasil, curtos e diretos.

## Forma e espaçamento

- Cards: raio **10 px** (status e lateral: **12 px**), borda 1 px, sem sombra nos painéis de áudio.
- Controles segmentados: raio 12 px por fora, 9 px por dentro.
- Espaçamento em múltiplos de 6 px: 6, 12, 18, 24.
- Janela padrão **1200 × 800**, mínimo **420 × 400**; área de áudio expansível, sem limite central de largura.
- Acima de 1000 px: áudio à esquerda, conexão à direita em painel de aproximadamente 340 px.
- Até 1000 px: módulos e conexão em uma coluna, com rolagem vertical.
- Os módulos se distribuem em até duas colunas; o seletor de perfis quebra linhas em larguras menores.

## Componentes

- **Card de status (hero):** bolinha de 14 px com brilho + título + detalhe + dois medidores
  de 14 px (entrada e saída tratada), lado a lado em telas grandes e empilhados em telas pequenas, valores RMS em dBFS e escala de −60 a 0 dBFS.
- **QR code:** cartão branco com sombra, em grupo próprio logo abaixo do card de status, visível
  só com a janela de pareamento aberta.
- **Seletor de perfil:** controle segmentado (Natural, Chamada, Podcast, Streaming, Original),
  com a descrição do perfil logo abaixo.
- **Módulos:** Limpeza (RNNoise, graves, gate), Timbre (de-esser, presença), Dinâmica (compressor) e Volume (automático/manual).
- **Conexão:** painel lateral com servidor, pareamento, dispositivos, inicialização e certificado; botão para copiar a impressão digital completa.
- **Listas:** `boxed-list` do libadwaita, superfícies grafite e borda fina.
- **Diálogos:** sempre com `HeaderBar` (X) e botão de ação explícito.

## Página do celular (`app/web`)

Mesmo fundo (`bg-top` → `bg-bottom`), mesma bolinha de status e mesmos raios. Botão principal na
cor `accent`. Deve funcionar com uma mão, com alvo de toque de pelo menos 48 px.

## Acessibilidade

Contraste mínimo 4,5:1 para texto; status nunca só por cor (sempre acompanhado do texto do
estado); animações respeitam `prefers-reduced-motion` na página web.

## Teste de voz

Painel abaixo dos perfis, com captura de até 30 s, duração por amostras e botões de
comparação original/tratado, parada e exportação WAV. Os botões quebram em linhas
em janelas pequenas. Reprodução ativa recebe destaque de seleção; gravação incompleta,
falhas de reprodução e exportação parcial são descritas em texto. Exportação usa o
seletor nativo e confirmação explícita para substituição dos dois arquivos. A reprodução
é exclusiva e sai no dispositivo padrão do sistema; o painel orienta o uso de fones.

## Retorno no fone

Painel junto ao teste A/B, com escolha explícita de saída, botão de atualização,
interruptor e volume independente de 0 a 100% (inicial 25%). Retorno inicia desligado.
A seleção permanece identificada por node/serial; se desaparecer, fica marcada como
indisponível. Não selecionar outro dispositivo automaticamente. Estados de preparação,
atividade e erro aparecem no painel sem substituir o estado principal da conexão.
Os avisos orientam o uso de fones e informam o atraso sem inventar medição de latência.
