# MicBridge: sistema de design

Fonte única das decisões visuais do app GTK4/libadwaita, da bandeja e da página do
celular. Ao mudar uma cor ou medida, atualize este arquivo junto.

## Princípios

1. **Coerente com o desktop.** Sistema é o padrão; claro, escuro e tema importado são escolhas explícitas. Em sessão Omarchy detectada, Sistema acompanha sua paleta automaticamente. Cores nativas e fonte do sistema prevalecem no modo Sistema.
2. **Monitoramento persistente.** Conexão, detalhes e medidores ficam numa barra fixa, fora da rolagem. Os controles de áudio rolam abaixo dela.
3. **Legibilidade.** Texto em dBFS com largura estável, contraste validado nos temas importados e alto contraste nativo. Fundos podem ser translúcidos; textos e medidores não recebem opacidade global.
4. **Movimento contido.** Sem piscar continuamente no conteúdo GTK; atividade aparece nos medidores e no anel do estado. Controles nativos respeitam as preferências do ambiente.
5. **Estrutura sóbria.** Superfícies sem sombras pesadas, bordas finas, raios 10–12 px e espaços em múltiplos de 6 px. Tipografia relativa à fonte do sistema.

## Layout e aparência

- Janela 1200×800, mínimo 420×400. Monitoramento ocupa a largura útil do topo, com 12 px de respiro abaixo da barra de título.
- Até 1100 px, painéis passam a uma coluna. Até 600 px, medidores são empilhados na barra fixa. Mesmo em 420×400 a área rolável mantém pelo menos 100 px com a fonte padrão.
- Paleta nativa usa as variáveis semânticas do libadwaita. O app não força azul/escuro globalmente.
- Temas JSON v1 trazem nome, modo, fundo, superfície, texto e destaque. Cores são hexadecimais, contraste de texto mínimo 4,5:1, importação limitada 64 KiB. Exemplo em `app/data/themes/graphite.json`.
- Paletas personalizadas têm prioridade local sobre cores globais; Sistema respeita as cores globais. Não escrever em configurações GTK/KDE/Omarchy.
- Opacidade 60–100%, padrão 100, aplicada ao fundo da área principal. Cards/controles preservam contraste. Alto contraste ignora paletas personalizadas/transparência.
- O modo Sistema detecta a sessão Omarchy pelos marcadores do ambiente e acompanha o `colors.toml` atual/legado por leitura assíncrona a cada 2 s; erro preserva a última paleta válida e mostra aviso. Paleta ausente usa o sistema.
- Aparência abre num PreferencesDialog adaptável e único por janela. Prévia é temporária: Aplicar salva atomicamente, Cancelar/Esc/fechar restaura o salvo; Restaurar padrão também é uma prévia. Operações de arquivo/salvamento não bloqueiam GTK.
- Restaurar padrão fica separado dos botões de importar/exportar por 12 px.
- Importação/exportação usa seletor nativo. Blur depende do compositor. GNOME/KDE seguem preferências disponíveis pelo StyleManager/portal, mantendo widgets GTK.
- A biblioteca requer GTK 4.16/libadwaita 1.6 para variáveis de cor modernas; não depende de plugin de tema.

Referências: [GNOME HIG](https://developer.gnome.org/hig/), [ToolbarView](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.ToolbarView.html), [StyleManager](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.StyleManager.html), [paletas Omarchy](https://github.com/basecamp/omarchy/blob/master/bin/omarchy-theme-color).

## Página do celular (`app/web`)

Mesmo fundo (`bg-top` → `bg-bottom`), mesma bolinha de status e mesmos raios. Botão principal na
cor `accent`. Deve funcionar com uma mão, com alvo de toque de pelo menos 48 px.

## Acessibilidade

Contraste mínimo 4,5:1 para texto; status nunca só por cor (sempre acompanhado do texto do
estado); animações respeitam `prefers-reduced-motion` na página web.

## Teste de voz

Painel de teste local, com captura de até 30 s, duração por amostras e botões de
comparação original/tratado, parada e exportação WAV. Os botões quebram em linhas
em janelas pequenas. Reprodução ativa recebe destaque de seleção; gravação incompleta,
falhas de reprodução e exportação parcial são descritas em texto. Exportação usa o
seletor nativo e confirmação explícita para substituição dos dois arquivos. A reprodução
é exclusiva e sai no dispositivo padrão do sistema; o painel orienta o uso de fones.

## Retorno no fone

Painel de monitoramento local, com escolha explícita de saída, botão de atualização,
interruptor e volume independente de 0 a 100% (inicial 25%). Retorno inicia desligado.
A seleção permanece identificada por node/serial; se desaparecer, fica marcada como
indisponível. Não selecionar outro dispositivo automaticamente. Estados de preparação,
atividade e erro aparecem no painel sem substituir o estado principal da conexão.
Os avisos orientam o uso de fones e informam o atraso sem inventar medição de latência.

O interruptor passa a “Ajuste em tempo real · retorno exclusivo”, com aviso visível
sobre a desconexão temporária de outros aplicativos da saída selecionada. Preparação
e restauração têm estados próprios; não permitir nova ativação durante restauração
ou com conexões pendentes. Exibir “Tentar restaurar conexões” quando for necessário,
inclusive sem celular conectado. Erro de restauração não desaparece ao desligar.

## Atuação dos efeitos

Os estados do gate e os ganhos do compressor/volume automático aparecem como
subtítulos dos respectivos controles. O gate distingue aberto, espera e atenuação;
valores de ganho usam dB e sinal explícito no ajuste automático. Estados textuais
independem de cor. Efeito desativado mostra “Desligado”; conexão inativa ou 300 ms
sem novos frames mostra “Sem áudio” nos efeitos habilitados. Reutilizar a atualização
existente de 20 Hz, sem criar outro timer nem reter widgets pelos indicadores.

## Organização dos painéis

Usar colunas independentes, com altura natural e intervalo de 18 px entre painéis;
não alinhar a próxima seção pela altura do maior painel de outra coluna. Acima de
1240 px úteis no estúdio, três colunas: perfil/limpeza/timbre; dinâmica/volume;
teste/retorno/QR.
Teste A/B e retorno ao vivo devem permanecer consecutivos na mesma coluna em
todas as larguras, formando a área de teste e monitoramento.
Entre 801 e 1240 px úteis, duas: tratamento de voz à esquerda, teste/retorno/QR à
direita. Até 800 px úteis, uma coluna. QR continua condicionado ao
estado do pareamento. Os mesmos widgets são reposicionados sem recriar controles.

Estúdio centralizado na área rolável direita, com limite de 1440 px; em duas colunas o limite é
1040 px e em uma, 584 px, incluindo margens. Evitar esticamento dos painéis em
monitores largos. Cabeçalho e corpo têm limite externo de largura de 1800 px;
medidores são empilhados até 600 px. Manter área de rolagem útil em 420×400.

## Conexão fixa

Conexão fica numa lateral esquerda de até 340 px, abaixo do status e fora de
qualquer ScrolledWindow. Apenas o estúdio à direita rola. As colunas respondem à
largura útil da área direita por um BreakpointBin, sem incluir a lateral.

Até 1180 px de largura ou 680 px de altura, recolher a lateral e mostrar o botão
Conexão no cabeçalho. Esse botão apresenta o mesmo painel num diálogo nativo,
com rolagem apenas quando necessária dentro do diálogo para alcançar os controles.
Ao retornar ao tamanho de desktop, fechar o diálogo e devolver o painel à lateral,
preservando o estado. QR inline permanece no estúdio; o botão de QR na conexão
continua abrindo a visualização dedicada.
