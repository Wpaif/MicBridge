# MicBridge: sistema de design

Fonte única das decisões visuais do app GTK4/libadwaita, da bandeja e da página do
celular. Ao mudar uma cor ou medida, atualize este arquivo junto.

## Princípios

1. **Calmo e escuro.** Superfícies escuras translúcidas, bordas finas, brilho só onde há
   significado (status e voz). O app usa sempre o tema escuro (`ColorScheme::ForceDark`): o
   vidro e os brilhos foram desenhados para ele e não funcionam bem no claro.
2. **Status em destaque.** A primeira coisa na janela é o estado da conexão, grande e com cor.
3. **Simples para leigos.** Um perfil de voz resolve; o resto fica em "Ajustes avançados".
4. **Movimento com propósito.** Só anima o que está vivo (conectando, erro, voz). Sem enfeite.
5. **Nativo.** Usa widgets e classes do libadwaita; o CSS refina, não reinventa.

## Cores

| Token | Valor | Uso |
|-------|-------|-----|
| `bg-top` / `bg-bottom` | `#15171e` / `#0d0f14` | Fundo da janela (degradê vertical) |
| `surface` | `rgba(255,255,255,.045)` | Cards e listas |
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

Fonte do sistema (Adwaita Sans/Cantarell). Título do status 26 px / peso 800; rótulos de seção
11 px, peso 700, espaçamento de letras +0,08 em, cor `text-dim`; corpo no tamanho padrão do
libadwaita. Textos em português do Brasil, curtos e diretos.

## Forma e espaçamento

- Cards: raio **16 px** (status em destaque: **20 px**), borda 1 px, sombra suave.
- Controles segmentados: raio 12 px por fora, 9 px por dentro.
- Espaçamento em múltiplos de 6 px: 6, 12, 18, 24.
- Janela padrão 560 × 780; conteúdo centralizado (o `PreferencesPage` limita a largura).

## Componentes

- **Card de status (hero):** bolinha de 14 px com brilho + título + detalhe + dois medidores
  finos (entrada e saída tratada).
- **QR code:** cartão branco com sombra, em grupo próprio logo abaixo do card de status, visível
  só com a janela de pareamento aberta.
- **Seletor de perfil:** controle segmentado (Natural, Chamada, Podcast, Streaming, Original),
  com a descrição do perfil logo abaixo; "Ajustes avançados" fica em um grupo separado.
- **Listas:** `boxed-list` do libadwaita, restilizadas (translúcidas, borda fina).
- **Diálogos:** sempre com `HeaderBar` (X) e botão de ação explícito.

## Página do celular (`app/web`)

Mesmo fundo (`bg-top` → `bg-bottom`), mesma bolinha de status e mesmos raios. Botão principal na
cor `accent`. Deve funcionar com uma mão, com alvo de toque de pelo menos 48 px.

## Acessibilidade

Contraste mínimo 4,5:1 para texto; status nunca só por cor (sempre acompanhado do texto do
estado); animações respeitam `prefers-reduced-motion` na página web.
