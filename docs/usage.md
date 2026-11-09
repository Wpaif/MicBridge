# Guia de uso

## Primeira conexão

1. Coloque PC e celular na mesma rede local e abra o MicBridge.
2. Na seção **Conexão**, habilite **Aceitar novo celular** se necessário (atalho `P`).
3. Escaneie o QR code da seção **Conexão** com o celular.
4. Autorize o microfone no navegador e toque em **Iniciar microfone**.
5. No Discord, OBS ou aplicativo de chamada, selecione **MicBridge** como entrada.

O navegador precisa aceitar a conexão HTTPS para disponibilizar o microfone.
Consulte [Certificados](security.md#certificados) para configurar a CA local e
conferir a impressão digital exibida no PC.

Na próxima utilização, abra o app no PC e a página no celular. O navegador
reconhecido pode reconectar sem escanear outro QR. Isso depende de manter as
credenciais do site e usar o mesmo endereço.

## Interface

A janela é aberta pelo [Quickshell](https://quickshell.org) e tem um trilho à
esquerda com quatro seções: **Voz**, **Perfil**, **Efeitos** e **Conexão**. No topo
do trilho fica o estado da conexão; embaixo, **Silenciar** e a folha de atalhos.
Uma bolinha ao lado da seção indica o que está ativo sem precisar abri-la
(gravação, retorno, fonte extra, alterações não salvas, pareamento aberto).

- **Voz:** medidores de entrada e saída, histórico de níveis e os cards
  **Retorno no fone**, **Teste de voz**, **Fonte extra** e **Calibração**.
- **Perfil:** perfis de fábrica e personalizados.
- **Efeitos:** a mesa de som com os efeitos (veja abaixo).
- **Conexão:** QR code, pareamento, celulares conhecidos, certificado e
  **Iniciar com o sistema**.

Tudo funciona pelo teclado; `?` ou `F1` abre a folha de atalhos. Os faders e
sliders ignoram a roda do mouse, para a rolagem da página não mexer no áudio.

As cores seguem o tema atual do Omarchy (`colors.toml`) e mudam junto com ele.
Fora do Omarchy a janela usa uma paleta escura padrão.

## Tratamento da voz

Comece com um dos perfis de fábrica:

| Perfil | Uso previsto |
|---|---|
| Natural | Processamento leve. |
| Chamada | Limpeza e clareza para reuniões; padrão inicial. |
| Podcast | Voz nivelada e encorpada. |
| Streaming | Gate mais firme para ruído de teclado e ambiente. |
| Original | Módulos de limpeza, timbre e dinâmica desativados; ganho manual e limitador permanecem. |

Refine na seção **Efeitos**, uma mesa de som com um canal por efeito, na ordem em
que o sinal passa por eles. Cada canal tem um botão ON/OFF embaixo e um LED que
acende com o efeito ligado:

| Canal | Efeito e controles |
|---|---|
| Graves | Passa-altas: atenua rumble e vibrações abaixo de ~80 Hz. |
| Ruído | RNNoise para atenuar ruído de fundo. |
| Gate | Atenua o sinal abaixo do limiar (fader), com sustentação para pausas curtas. O LED fica verde aberto, amarelo em espera e vermelho fechado. |
| De-esser | Suaviza sibilantes. |
| Presença | Realce de clareza em ~3 kHz. |
| EQ | Equalizador de 3 bandas: graves (120 Hz), médios (1 kHz) e agudos (6 kHz), de −12 a +12 dB. Começa desligado e plano; duplo clique num fader volta a banda a 0 dB. Os atalhos ao lado dos faders ligam o EQ e posicionam as bandas (veja abaixo). |
| Ganho | **AUTO** ajusta o volume e congela nas pausas; o medidor mostra o ajuste. Em **MANUAL** vale o fader de ganho. |
| Compressor | Reduz diferenças de dinâmica (faders de limiar e razão). O medidor GR mostra quanto está reduzindo; um makeup automático devolve o volume médio, para a voz não ficar mais baixa. |
| Saída | Níveis de entrada e saída e o silenciar (**AO VIVO** / **MUDO**). |

Atalhos do EQ, um ponto de partida para ajustar depois nos faders:

| Atalho | Para | Graves / médios / agudos |
|---|---|---|
| Plano | Voltar ao som sem equalização. | 0 / 0 / 0 dB |
| Clarear | Voz abafada: celular no bolso, máscara, tecido na frente. | −3 / +1 / +6 dB |
| Encorpar | Voz fina: celular longe ou microfone pequeno. | +5 / 0 / −2 dB |
| Secar | Sala com eco, voz embolada. | −5 / −3 / +2 dB |

O atalho fica destacado enquanto as bandas baterem com ele; mexer num fader o
desmarca. Como o resto do EQ, o resultado entra no perfil ao salvar.

Os medidores mostram **—** sem áudio recente, o que indica ausência de frames, não
necessariamente uma falha na conexão. Eles não equivalem a um diagnóstico geral.

**CLIP** acende em vermelho nos medidores (na Voz e no canal Saída) e fica aceso
por 3 segundos depois da última saturação:

- **Entrada:** o áudio já chega saturado do celular, e nenhum efeito conserta
  isso. Afaste o celular da boca ou fale mais baixo.
- **Saída:** o limitador está cortando mais de 6 dB para não passar do teto, sinal
  de ganho alto demais. Baixe o ganho manual ou as bandas do EQ.

Se a mesa não couber na janela, ela rola na
horizontal pela roda do mouse, pelo touchpad ou pela barra embaixo dos canais.

## Calibração

O card **Calibração** (seção **Voz**) ajusta o tratamento ao seu ambiente em 13
segundos, com o celular conectado e na posição em que você vai usá-lo:

1. **Silêncio (5 s):** fique quieto, com o ambiente como costuma estar (ventilador,
   ar-condicionado e afins ligados).
2. **Leitura (8 s):** leia o texto mostrado em voz normal, como numa chamada.

O app mede o ruído e a sua voz no ponto em que o gate decide, depois do filtro de
graves e da redução de ruído, e mostra o que mudaria:

| Ajuste | Regra |
|---|---|
| Gate | Liga, com o limiar 6 dB acima do ruído e pelo menos 18 dB abaixo da voz, para não cortar sílabas fracas. |
| Redução de ruído | Liga se o ruído do ambiente passar de −62 dBFS; nunca desliga se já estava ligada. |
| Ganho manual | Só com o volume automático desligado: leva a voz ao nível-alvo (−18 dBFS). |

Nada muda até você tocar em **Aplicar**. Aplicado, o resultado aparece como
alterações não salvas do perfil atual; use **Salvar alterações** ou **Salvar como
novo…** para guardá-lo. Se a voz ficar menos de 10 dB acima do ruído, o card avisa
para aproximar o celular ou procurar um lugar mais silencioso. Durante a medição,
o filtro de graves e a redução de ruído ficam ligados e voltam ao estado anterior
ao terminar ou cancelar.

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
volume do retorno e fonte extra não fazem parte deles. O equalizador faz parte;
perfis salvos antes dele abrem com o EQ desligado e plano.

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

No card **Retorno no fone** (seção **Voz**), escolha uma saída e ative **Ouvir a
voz tratada**. O retorno inicia desligado, com volume local de 25%. O app não troca
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

**Silenciar** (no trilho, atalho `M`, ou **MUDO** no canal Saída da mesa) silencia
somente a saída **MicBridge** para os outros programas. O botão fica destacado e o
medidor de saída apresenta zero.

Entrada, processamento, retorno no fone e gravação de teste continuam ativos.
Desmutar mantém conexão e configurações. O app inicia desmutado; essa escolha
não é salva entre execuções. Áudio já entregue aos buffers do sistema ainda pode
ser ouvido por um breve instante.

## Fonte extra

O card **Fonte extra** (seção **Voz**) manda o som de um app junto com a sua voz
para o microfone virtual, por exemplo uma aba do navegador tocando um vídeo numa
call. Ele lista os apps que estão tocando som; escolha um e ajuste o volume. Você
continua ouvindo o app no fone normalmente.

Escolher o próprio app da chamada devolve o som da call para ela mesma (eco); o
card avisa quando isso acontece. Tocar de novo no app selecionado desliga a fonte.

## Bandeja e encerramento

Fechar a janela apenas a esconde; o MicBridge continua rodando e o microfone
virtual segue ativo. Use **Sair** na bandeja para encerrar o app e restaurar as
conexões de áudio pendentes pelo fluxo normal de parada. **Iniciar com o sistema**,
na seção **Conexão**, configura a abertura escondida na bandeja.
