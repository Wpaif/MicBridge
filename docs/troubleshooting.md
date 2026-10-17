# Solução de problemas

## A página não abre ou não conecta

Confira o endereço no painel **Conexão**, se PC e celular estão na mesma rede e
se o aplicativo está aberto. O listener HTTPS usa o IP descoberto na inicialização;
se o IP mudou, reabra o app. Redes de convidados podem isolar clientes entre si.

Sem resolução de `micbridge.local`, use o IP exibido. Avahi é opcional, mas mudar
de nome para IP também muda a origem do site e suas credenciais de navegador.

Se a página abre mas o áudio não chega, confira a permissão de microfone e a
negociação WebRTC. HTTPS acessível na porta 3443 não comprova que o transporte
de mídia esteja permitido. Não há STUN configurado para contornar a rede local.

## O navegador não libera o microfone

Confirme a permissão do site e o certificado HTTPS. Consulte
[Segurança e dados](security.md#certificados). Limpar os dados do site apaga suas
credenciais de pareamento; nesse caso, habilite **Aceitar novo celular** e pareie
novamente. Outro celular conectado pode impedir uma nova conexão.

## O áudio para quando a tela do celular bloqueia

A página tenta manter a tela acordada durante a transmissão. O navegador ou o
sistema ainda pode suspender o microfone em segundo plano ou com a tela bloqueada.
Teste com a página em primeiro plano para distinguir suspensão de uma falha no DSP.

## A chamada não recebe áudio

Selecione **MicBridge** na entrada do programa de destino e confira o mute
do MicBridge e do próprio programa. Verifique os medidores de entrada e saída.
O retorno no fone continuar audível durante o mute é comportamento previsto.

Confirme que PipeWire e suas ferramentas estão disponíveis. O app tenta recriar
o microfone virtual se ele desaparecer. Para observar mensagens, execute o binário
pelo terminal, conforme [Desenvolvimento](development.md).

## A voz corta ou está excessivamente processada

Compare a mesma frase no teste A/B, incluindo trechos baixos e pausas curtas.
Teste o gate desligado e depois a redução de ruído desligada, uma mudança por vez.
O perfil Streaming é mais agressivo com ruído; Natural permite uma comparação
com processamento leve. Confira também a supressão no aplicativo de chamada.

Grave uma amostra do problema atual no teste A/B para distinguir cortes na
captura de alterações causadas pelo processamento.

## O retorno não toca no fone ou foi interrompido

Atualize a lista de saídas e selecione o dispositivo correto, inclusive no caso
de Bluetooth. O app não escolhe automaticamente outra saída se o dispositivo
selecionado desaparecer. Reative o retorno após reconectar o celular ou o fone.

Mensagens de bloco incompatível, fila cheia ou atraso indicam que o retorno foi
interrompido para não acumular áudio indefinidamente. O microfone virtual não
deve depender da reprodução local. Compare também uma saída com fio para separar
latência do transporte Bluetooth de problemas de processamento.

## Outros programas ficaram sem som durante o ajuste

O retorno exclusivo desconecta outros links da saída selecionada enquanto está
ativo. Ao desligar, aguarde **Restaurando conexões…**. Se houver pendências, use
**Tentar restaurar conexões**. Não apague o registro local de roteamento.
Dispositivos/portas removidos ou substituídos podem impedir a restauração original.

## Não consigo salvar um perfil ou tema

Confira a mensagem do editor e a possibilidade de escrita no diretório de dados.
Perfis exigem nome único de 1–60 caracteres e têm limite de 32 personalizados.
Perfis de fábrica não são sobrescritos: use **Salvar como novo…**.

O app preserva uma biblioteca inválida/incompatível e bloqueia sua edição.
Antes de corrigir preferências manualmente, feche o app e faça uma cópia de
`settings.json`. Não apague o diretório inteiro: ele também contém pareamento,
certificados e o registro de restauração de áudio.

## A conexão sumiu da lateral ou a bandeja não aparece

Em janela estreita ou baixa, abra **Conexão** pelo botão de rede no cabeçalho.
Esse é o modo compacto, não perda do painel. A bandeja depende do suporte
StatusNotifier do desktop. Fechar a janela só a esconde; **Sair** na bandeja encerra.

## O tema não acompanha Omarchy ou o fundo não fica desfocado

Selecione **Seguir o sistema** e confirme que a sessão é reconhecida como Omarchy.
GNOME/KDE não usam automaticamente paletas Omarchy deixadas no disco. Um erro de
leitura mantém a última paleta válida; o app informa quando usa o fallback nativo.
Alto contraste desativa transparência/paleta customizada. Desfoque depende do
compositor e não é configurado pelo MicBridge.
