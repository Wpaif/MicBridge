# Design da interface

Este é o documento de referência para a interface atual. Ela é QML, aberta pelo
[Quickshell](https://quickshell.org): a janela está em `app/ui/shell.qml` e os
componentes ficam ao lado, em `app/ui/*.qml`. O daemon (`app/src/ipc.rs`) envia o
estado e recebe comandos; a interface não guarda regras de áudio próprias.

## Princípios

O usuário deve conseguir identificar se há conexão, se o áudio chega e se a
saída está silenciada sem procurar controles. Use textos curtos em português e
estados textuais além de cores: um LED aceso sempre vem com uma palavra ou valor.

Animação indica operação/estado, não decoração. Durante operações assíncronas,
apresente progresso e impeça ações conflitantes até a conclusão; não confirme uma
operação apenas porque houve um clique.

## Estrutura

- **Trilho à esquerda:** estado da conexão no topo, as seções no meio e, embaixo,
  a folha de atalhos e **Silenciar**. Uma bolinha ao lado da seção mostra o que
  está ativo nela.
- **Painel à direita:** só a seção escolhida, com título e rolagem vertical própria.

| Seção | Conteúdo |
|---|---|
| Voz | Medidores, histórico de níveis e os cards Retorno no fone, Teste de voz, Fonte extra e Calibração. |
| Perfil | Perfis de fábrica e personalizados, com salvar, renomear e excluir. |
| Efeitos | Mesa de som: um canal (`Strip`) por efeito, na ordem do sinal. |
| Conexão | QR, pareamento, celulares conhecidos, inicialização e certificado. |

Recursos com poucos controles viram um `Card` dentro de uma seção, não uma seção
própria: uma página quase vazia no trilho é pior que um card a mais na Voz. Os
cards ficam lado a lado na janela larga (`win.wide`, a partir de 1000 px) e
empilhados na estreita.

Formulários usam uma coluna legível (até 720 px); páginas com gráfico ou mesa usam
a largura da tela cheia (até 1200 px). A mesa de efeitos ocupa a altura disponível
e rola na horizontal quando não cabe, com roda, touchpad ou barra própria.

## Componentes

| Componente | Uso |
|---|---|
| `Toggle`, `Slider`, `Chip`, `Button` | Controles de linha nos formulários. |
| `Card` | Agrupa um recurso com título e um ponto de atividade. |
| `Strip`, `Fader`, `VMeter` | Canal da mesa, fader vertical e medidor vertical. |
| `Meter`, `History` | Nível em dBFS e histórico de níveis da seção Voz. |
| `Confirm`, `NameField` | Confirmações e nomes de perfil dentro da própria página. |

Faders e sliders não reagem à roda do mouse: a rolagem passa para a página sem
alterar o áudio. Durante o arraste mostram o valor local e depois seguram esse
valor até o daemon confirmar. Faixas com sinal (EQ) acendem a partir do zero, e o
duplo clique volta ao valor de repouso.

Confirmações (substituir WAVs, descartar alterações, excluir perfil, esquecer
celulares) aparecem na própria página, não em janelas separadas.

## Teclado

Tudo deve funcionar pelo teclado. Tab e ↑↓ andam entre controles; o painel rola
até o controle focado. Ctrl+Tab, Alt+↑↓ e Alt+1…4 trocam de seção. Esc solta o
foco e, de novo, fecha a janela. Ao criar um atalho novo, acrescente-o à folha de
atalhos (`?` / F1) em `shell.qml`.

## Cores

`Theme.qml` lê a paleta do Omarchy em `colors.toml` e a relê quando o tema muda.
Fora do Omarchy fica a paleta escura padrão do próprio arquivo. Use os tokens do
`Theme` (`background`, `surface`, `foreground`, `muted`, `accent`, `ok`, `warn`,
`urgent`), nunca cores fixas nos componentes.

## Página do celular

A página em `app/web` precisa funcionar em tela pequena, com ação principal
visível, estado compreensível e controles de toque confortáveis. Não exponha
detalhes internos de PipeWire, filas ou DSP no fluxo de iniciar/parar a captura.

## Verificação visual

Teste a janela em meia tela e em tela cheia, com tema claro e escuro do Omarchy.
Verifique foco, cortes de texto, rolagem e estados de operação. Captura com estado
simulado não comprova funcionamento de rede ou áudio; registre à parte o que
exigir celular, PipeWire ou fone.
