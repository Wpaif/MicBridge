# Guia de uso

## Primeira conexão

1. Coloque PC e celular na mesma rede local e abra o MicBridge.
2. No painel **Conexão**, habilite **Aceitar novo celular** se necessário.
3. Escaneie o QR code com o celular. O botão de QR na conexão abre uma
   visualização dedicada; o QR inline aparece durante a janela de pareamento.
4. Autorize o microfone no navegador e toque em **Iniciar microfone**.
5. No Discord, OBS ou aplicativo de chamada, selecione **Microfone_VDO** como entrada.

O navegador precisa aceitar a conexão HTTPS para disponibilizar o microfone.
Consulte [Certificados](security.md#certificados) para configurar a CA local e
conferir a impressão digital exibida no PC.

Na próxima utilização, abra o app no PC e a página no celular. O navegador
reconhecido pode reconectar sem escanear outro QR. Isso depende de manter as
credenciais do site e usar o mesmo endereço.

## Interface e rolagem

Status, medidores de entrada/saída e mute ficam fixos no topo. A conexão fica
fixa à esquerda, abaixo do status; o estúdio à direita tem rolagem independente.
Em janelas estreitas ou baixas, o botão de rede **Conexão** na barra superior abre
o mesmo painel num diálogo.

Os painéis se reorganizam conforme o espaço útil: tratamento de voz, teste A/B
e retorno permanecem agrupados por contexto. A/B e retorno ficam consecutivos.
Os controles têm largura limitada para não esticarem excessivamente ao maximizar.

Use arraste ou setas do teclado nos deslizantes. A roda do mouse e a rolagem
sobre eles continuam rolando o painel, sem modificar os valores, mesmo com foco.

## Tratamento da voz

Comece com um dos perfis de fábrica:

| Perfil | Uso previsto |
|---|---|
| Natural | Processamento leve. |
| Chamada | Limpeza e clareza para reuniões; padrão inicial. |
| Podcast | Voz nivelada e encorpada. |
| Streaming | Gate mais firme para ruído de teclado e ambiente. |
| Original | Módulos de limpeza, timbre e dinâmica desativados; ganho manual e limitador permanecem. |

Refine nos módulos **Limpeza**, **Timbre**, **Dinâmica** e **Volume**:

| Controle | Efeito |
|---|---|
| Redução de ruído | RNNoise para atenuar ruído de fundo. |
| Filtro de graves | Atenua rumble e vibrações graves. |
| Noise gate e limiar | Atenua o sinal abaixo do limiar, com sustentação para pausas curtas. |
| Suavizar chiados | De-esser para sibilantes. |
| Realçar clareza da voz | Realce de presença. |
| Compressor, limiar e razão | Reduz diferenças de dinâmica. |
| Volume automático | Ajusta ganho e congela nas pausas. |
| Ganho manual | Atua quando o volume automático está desligado. |

Gate, compressor e volume automático exibem sua atuação. **Sem áudio** indica
ausência de frames recentes, não necessariamente uma falha na conexão. Esses
indicadores não equivalem a um diagnóstico geral nem a um alerta de clipping.

## Perfis personalizados

Após ajustar a voz, use **Salvar como novo…** e escolha um nome. Você pode
selecionar, atualizar, renomear e excluir os personalizados. Os cinco perfis de
fábrica são protegidos.

**Alterações não salvas** identifica ajustes que só estão ativos na sessão.
**Salvar alterações** atualiza o personalizado selecionado. Renomear não salva
ajustes pendentes. Trocar de perfil pede confirmação antes de descartá-los.
Excluir o perfil ativo retorna a **Chamada**.

O app restaura o último perfil selecionado com seus ajustes salvos ao reabrir.
São aceitos até 32 personalizados, com nomes únicos de até 60 caracteres.
Os perfis são locais, sem importação/exportação. Mute, retorno, dispositivos,
volume do retorno e aparência não fazem parte deles.

Uma falha de escrita preserva os dados anteriores e mostra o erro. Se a biblioteca
estiver inválida ou incompatível, o app preserva o arquivo e bloqueia as ações de
perfil, mantendo os ajustes manuais disponíveis. Veja [Solução de problemas](troubleshooting.md).

## Teste de voz e comparação A/B

Use **Gravar teste**, fale e depois **Parar gravação**. A captura termina após
30 segundos de áudio recebido. **Ouvir original** reproduz o PCM recebido antes
do tratamento local; **Ouvir tratado** reproduz o resultado do DSP.

A reprodução usa a saída padrão do sistema. Use fones. Um novo teste substitui
o anterior; desconectar finaliza o trecho, e reconectar não continua a gravação.
Nenhum teste é salvo automaticamente.

**Exportar WAVs** grava original e tratado em arquivos separados, mono, 48 kHz,
PCM de 16 bits, com confirmação antes de substituir destinos existentes. Uma
falha informa se apenas um arquivo foi concluído.

O teste permite comparar timbre, volume e cortes; não mede a latência total.
O atraso interno do RNNoise não é compensado entre as versões. Uma fila de
captura cheia resulta em **Teste incompleto**, sem bloquear o áudio ao vivo.

## Retorno no fone

Escolha uma saída em **Retorno no fone** e ative **Ajuste em tempo real · retorno
exclusivo**. O retorno inicia desligado, com volume local de 25%. O app não troca
automaticamente para outra saída se a escolhida desaparecer.

Durante o ajuste, o MicBridge mantém somente seu retorno conectado àquela saída,
como ao reorganizar links no qpwgraph. Outros aplicativos ficam temporariamente
desconectados dessa saída. Você ouve a própria voz e ajusta os efeitos em tempo
real; chamadas e gravações continuam recebendo o áudio tratado normalmente.

**Preparando saída exclusiva…** e **Restaurando conexões…** indicam transições;
os controles envolvidos ficam bloqueados até a conclusão. Ao parar, o app tenta
restaurar os links válidos. Use **Tentar restaurar conexões** se houver pendência.
O registro local permite tentar recuperar links na próxima abertura após uma
interrupção abrupta. Portas ou dispositivos substituídos não são religados como
se fossem os anteriores.

Ouvir A/B encerra o retorno; ativar o retorno encerra a reprodução A/B. Gravar
um teste pode continuar com retorno ativo. Desconexão, troca/remoção de saída,
falha de reprodução ou atraso excessivo podem encerrá-lo. Há latência de
monitoramento, especialmente com Bluetooth; use fones para evitar realimentação.

## Mute

**Silenciar microfone** silencia somente a saída **Microfone_VDO** para os outros
programas. O botão fica destacado e o medidor de saída apresenta zero.

Entrada, processamento, retorno no fone e gravação de teste continuam ativos.
Desmutar mantém conexão e configurações. O app inicia desmutado; essa escolha
não é salva entre execuções. Áudio já entregue aos buffers do sistema ainda pode
ser ouvido por um breve instante.

## Aparência

Abra **Aparência** na barra superior. Há **Seguir o sistema**, **Claro**, **Escuro**
e **Personalizado**. A prévia só é salva ao **Aplicar**; cancelar/fechar restaura
o que estava salvo. **Restaurar padrão** também é uma prévia até aplicar.

Opacidade de fundo varia de 60% a 100%, sem tornar textos e medidores translúcidos.
Alto contraste usa cores nativas e fundo opaco. Desfoque depende do compositor;
o aplicativo não o configura nem altera o tema global do desktop.

Temas visuais podem ser importados/exportados como paletas JSON v1. Veja o
[exemplo Graphite](../app/data/themes/graphite.json). Limite de 64 KiB, cores
`#RRGGBB` e contraste mínimo 4,5:1 do texto sobre fundo e superfície.
Isso é independente dos perfis de áudio, que não têm importação/exportação.

Em Omarchy detectado, **Seguir o sistema** acompanha sua paleta automaticamente.
A detecção considera a sessão ativa; arquivos antigos de Omarchy não devem mudar
o comportamento em GNOME/KDE. A paleta é verificada a cada dois segundos e erros
preservam a última válida. Sem paleta válida, o app usa o visual nativo com aviso.
Os caminhos estão em [Arquitetura](architecture.md#arquivos-do-aplicativo).

## Bandeja e encerramento

Fechar a janela apenas a esconde. Use **Sair** na bandeja para encerrar o app e
restaurar as conexões de áudio pendentes pelo fluxo normal de parada.
**Iniciar com o sistema** configura a abertura escondida na bandeja.
