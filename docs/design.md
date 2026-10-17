# Design da interface

Este é o documento de referência para a interface atual. A implementação está
em `app/src/*_ui.rs`, `main.rs`, `widgets.rs`, `studio_layout.rs`,
`connection_sidebar.rs`, `theme.rs` e `app/data/style.css`.

## Princípios

O usuário deve conseguir identificar se há conexão, se o áudio chega e se a
saída está silenciada sem procurar controles na área rolável. Use textos curtos
em português, estados textuais além de cores e widgets nativos de GTK/libadwaita.

Evite esticar controles ao maximizar. Prefira largura limitada, agrupamento por
função e altura natural dos painéis. Animação indica operação/estado, não decoração.
Durante operações assíncronas, apresente progresso e impeça ações conflitantes
até a conclusão; não confirme uma operação apenas porque houve um clique.

## Estrutura fixa e estúdio

- Barra de título: ações gerais e acesso à aparência.
- Topo fixo: status, medidores de entrada/saída e mute.
- Esquerda, abaixo do status: conexão fixa, sem ScrolledWindow, até 340 px.
- Direita: estúdio com rolagem própria e colunas independentes.

Cabeçalho e corpo têm limite externo de 1800 px. O estúdio tem limite de 1440 px
para três colunas, 1040 px para duas e 584 px para uma, incluindo suas margens.
Use 18 px entre painéis, mantendo espaçamentos internos próprios dos controles.

As colunas dependem da **largura útil do estúdio**, não da janela inteira:

| Largura útil | Agrupamento |
|---|---|
| Acima de 1240 px | Perfil/limpeza/timbre; dinâmica/volume; A/B/retorno/QR. |
| 801–1240 px | Tratamento; A/B/retorno/QR. |
| Até 800 px | Uma coluna. |

A/B e retorno devem ser consecutivos em qualquer modo. Não use linhas homogêneas
que deixem lacunas abaixo de painéis mais baixos. Reposicione os mesmos widgets,
preservando estado, sinais e foco.

Até 1180 px de largura da janela ou 680 px de altura, recolha a lateral e mostre
o botão **Conexão**. Ele abre o mesmo painel num diálogo nativo. O diálogo pode
rolar quando necessário para alcançar os controles; a lateral de desktop não.
Ao crescer a janela, feche o diálogo e devolva o painel à lateral.

Até 600 px da janela, empilhe os medidores. Preserve uma área útil de rolagem em
420×400. QR inline continua condicionado ao pareamento; o botão de QR da conexão
mantém a visualização dedicada.

## Controles e acessibilidade

Use `PreferencesGroup`, `ActionRow`, `SwitchRow`, `ComboRow` e componentes nativos
adequados. `ComboRow` deve estar numa ListBox para navegação e foco corretos.
Reutilize `widgets::slider`: roda/gestos de rolagem devem passar ao painel sem
alterar valores; arraste, setas e atualizações programáticas continuam funcionando.

O ganho manual só fica disponível com volume automático desligado. Gate,
compressor e volume automático mostram atuação ou estados **Desligado** e
**Sem áudio**. Esses textos não substituem os medidores nem um diagnóstico geral.

Mute usa texto e destaque, sem depender apenas de cor. Botões só com ícone devem
ter tooltip e nome acessível. Use fonte/tamanho relativos nos rótulos dos medidores.
Diálogos devem oferecer fechamento claro e navegação por teclado.

## Aparência e CSS

`style.css` concentra estrutura e classes `mb-*`. `theme.rs` gera a paleta aplicada
ao escopo `.mb-window`, sem modificar preferências globais do desktop. Os modos
são sistema, claro, escuro e personalizado. Alto contraste usa cores nativas e
superfícies opacas. Transparência afeta fundos, não textos e controles.

Use `add_css_class` na janela para preservar classes nativas como `csd` e
`background`. Não copie propriedades de CSS de navegador, como `backdrop-filter`
ou layout flex/grid, para o CSS GTK. Desfoque pertence ao compositor.

Omarchy é detectado pela sessão; **Seguir o sistema** já deve acompanhá-lo.
Não crie uma opção separada de integração nem altere o tema global. Prefira
integração nativa GTK também no KDE, sem prometer reprodução de controles Qt.

## Página do celular

A página em `app/web` precisa funcionar em tela pequena, com ação principal
visível, estado compreensível e controles de toque confortáveis. Não exponha
detalhes internos de PipeWire, filas ou DSP no fluxo de iniciar/parar a captura.

## Verificação visual

Teste janela pequena, intermediária, maximizada, claro/escuro, alto contraste e
diálogos. Verifique foco, cortes de texto, área de rolagem e estados de operação.
Use os testes e capturas documentados em [Desenvolvimento](development.md),
sem confundir status simulado com validação de áudio ou rede.
