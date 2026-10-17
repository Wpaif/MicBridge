# Validação do retorno exclusivo

Branch `fix/bluetooth-monitor`, base `0fe277b`. Spec e plano aprovados em
`docs/superpowers/{specs,plans}/2026-10-16-exclusive-monitor*`.
Execução direta, TDD por etapa, revisão independente da branch completa antes do merge.

## Comportamento

O ajuste conecta o retorno à saída escolhida, registra os links concorrentes antes
de removê-los e confirma o grafo. A atividade só é publicada após confirmar os
links corretos. Novos concorrentes são removidos durante o ajuste; quatro ciclos
sem convergência interrompem o modo e iniciam rollback. Outras saídas/fontes e o
microfone virtual não fazem parte da operação.

Desligamento, troca de modo, desconexão e saída normal passam pelo encerramento do
pw-cat e restauração dos links válidos. Pendências ficam visíveis, impedem outro
modo local e podem ser repetidas. Snapshot/controle de links não roda no pump/GTK.

O registro `monitor-routing.json`, no diretório de dados do MicBridge, é escrito
atomicamente com sync antes de remover conexões. Uma interrupção abrupta deixa a
recuperação para a próxima abertura. Cookie do servidor e serial de nodes/portas
impedem restaurar endpoints de outra sessão. Links já existentes são idempotentes;
objetos indisponíveis são contabilizados e não são recriados com outro nome/ID.

## TDD e correções durante a execução

- Grafo: três testes RED→GREEN, com duas aplicações concorrentes, outro destino,
  fontes, canais/nomes ambíguos, serial reutilizado, core diferente e passive.
- Backend/registro: quatro testes RED→GREEN para argumentos/identidades, confirmação
  por snapshot, exit/timeout/limite de 8 MiB, cancelamento, gravação atômica e lock.
  Um teste adicional RED→GREEN cobre eventos, EOF e recolhimento do listener.
- Controlador: cinco testes RED→GREEN para ordem registrar/remover/confirmar,
  concorrentes religando, rollback parcial/retry, cancelamento em consulta e recovery.
- Integração: três testes RED→GREEN para esperar roteamento, PCM independente,
  confirmação de parada com erro e encerramento assíncrono. Dois casos adicionais
  cobrem bloqueio de A/B por restauração pendente e cancelamento/falha de registro.
- A regressão adicional de cancelamento mostrou uma remoção após pedir parada:
  Monitor agora invalida sua lease no chamador, antes de o worker processar Stop.
- O select de preparação revelou que Child.wait fecha stdin; o worker agora toma
  posse de stdin antes do select e mantém o pipe até encerrar a transmissão.
- Um borrow de watch mantido durante send_replace causou bloqueio no controlador;
  a cópia do estado é obtida antes da escrita, com teste de conclusão/cleanup.
- Fixtures de processo usam nonce para evitar colisão de diretórios quando IDs de
  processos se repetem entre execuções interrompidas.
- Apresentação e teste GTK RED→GREEN: aviso, preparação/exclusividade, restauração,
  erro/retry e liberação da janela durante consulta pendente.
- A regressão SHA-256 do PCM dos cinco perfis permanece idêntica à base.

## Decisões de implementação

O lock permanece aberto durante a vida do controlador, inclusive no estado ocioso,
em vez de fechar/reabrir após cada recuperação. A mudança evita disputa transitória
na reaquisição e mantém um único dono do registro; outra instância/debug com o mesmo
diretório de dados precisa aguardar o encerramento da primeira.

Testes antigos do worker de PCM usam um bypass de roteamento injetado e exclusivo
de builds de teste. Os testes novos exercitam o controlador real com grafo controlado.
Nenhum teste comum ou gráfico acessa pw-link real ou o registro do usuário.
O teste Bluetooth explicitamente ignorado usa os serviços reais e o registro de
produção, permitindo recuperação na próxima abertura até se o teste for interrompido.

Os comandos CLI revalidam identidades antes e depois das alterações. O grafo do
PipeWire é concorrente: uma ligação nova pode existir brevemente até a notificação/
auditoria (250 ms) e sua remoção. Não há promessa de uma transação global instantânea
entre aplicativos. Os erros de operação ou perda da observação interrompem o ajuste.

## Verificação real no QCY MeloBuds Pro

Foi criado um concorrente temporário silencioso e usado o destino Bluetooth por
serial. O teste confirmou exclusividade, enviou três segundos de tom de 440 Hz com
amplitude de entrada 0,04 e retorno em 25%, capturando o monitor da saída somente
no intervalo exclusivo. Pico capturado **0,010000**, amplitude estimada em 440 Hz
**0,005328**. Após a parada, os **seis links originais** foram verificados.
A captura e os processos temporários foram encerrados; há cleanup mesmo em panic.

A captura usa `stream.capture.sink`, conforme a [documentação oficial do PipeWire](https://docs.pipewire.org/group__pw__keys.html).
Os testes do grafo não substituem a escuta real da voz do celular; essa validação
continua manual. Não medir nem apresentar esse ensaio como latência total.

## Comandos

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```

Executar cada teste gráfico em processo separado, adicionando
`-- --ignored --test-threads=1`: `exclusive_panel`,
`monitor_ui::tests::panel_controls`, `effect_ui::graphical_tests` e
`recording_ui::tests::panel_actions`.

Teste real, com autorização de acesso ao PipeWire, saída temporariamente exclusiva
e tom audível baixo:

```sh
cargo test --offline --manifest-path app/Cargo.toml bluetooth_live_monitor -- --ignored --nocapture
```

Resultado antes da revisão: **104 aprovados**, cinco testes excluídos por padrão
(quatro gráficos e um Bluetooth). Build offline, fmt e diff aprovados. Os testes
excluídos são executados explicitamente e registrados no fechamento da branch.

## Fechamento da revisão

A revisão independente pediu três correções importantes, reproduzidas em RED:
parada durante a revalidação de um concorrente ainda removia um link; links em
estado `error` contavam como conectados; o último ciclo podia confirmar exclusividade
com retorno ausente. Os testes agora passam com revalidação da lease antes do
comando destrutivo (inclusive após snapshot interno do backend), estado de falha
separado da existência do link, recuperação pendente para links com erro e confirmação
completa da identidade/conexão do retorno em cada saída bem-sucedida.

Uma regressão adicional reproduziu `Disabled` durante consulta pendente na parada;
o estado `Restoring` agora permanece até concluir a limpeza. O teste do backend
cobre também cancelamento durante sua própria consulta antes de lançar `pw-link`.

Suíte após correções: **108 aprovados, zero falhas, cinco ignorados**. Build offline,
fmt e diff verificados; teste gráfico do retorno exclusivo executado novamente.
A tentativa adicional de captura da janela completa não gerou imagem (sem nó de
renderização); havia outra instância ocupando a porta HTTPS. Os testes GTK continuam
sendo a evidência da interface, sem encerrar a instância existente.

## Reabertura após falha no uso real (16/10/2026)

O usuário observou “Retorno interrompido: não acompanhou o áudio ou recebeu um
bloco incompatível”. A integração foi desfeita na `main` com os reverts `092eb45`
e `1285d9e`; a árvore da main voltou à base `0fe277b`. A correção fica na branch
`fix/bluetooth-monitor`, sem novo merge enquanto aguarda validação manual.

Causa reproduzida: `server.rs` aceita 5760 amostras mono por pacote Opus (120 ms).
`submit_processed` divide esse pacote em doze blocos de 480 amostras, mas a fila
comportava seis (60 ms). Logo um pacote válido podia provocar sobrecarga antes de
o consumidor ter oportunidade de rodar. O teste anterior tratava essa falha como
esperada, em vez de exigir suporte a todo o intervalo aceito pelo decoder.

TDD: testes do pacote máximo, de duas entregas consecutivas sem escalonamento do
consumidor e da reprodução integral no worker falharam com `Failed` antes da
correção. A fila agora comporta 24 blocos (240 ms), suficientes para dois pacotes
máximos. O limite continua finito, não bloqueia o pump e ainda encerra o retorno
se o consumidor não acompanhar; não foi introduzida fila ilimitada nem descarte
silencioso. Esse é o limite do buffer interno; não é uma medição da latência total
do Bluetooth. DSP e PCM principal permanecem idênticos.

Verificação: **110 testes aprovados, cinco ignorados**, build offline, fmt e diff
aprovados. O teste Bluetooth foi alterado para enviar 25 pacotes de 120 ms em três
segundos (o teste anterior enviava apenas pacotes de 20 ms). A nova tentativa real
falhou na pré-condição **“fone Bluetooth indisponível”**, antes de alterar conexões
ou emitir áudio. A evidência Bluetooth do fechamento anterior não valida esta
correção: aguarda fone disponível e escuta manual do celular.

## Controles durante transições (16/10/2026)

O usuário confirmou que o retorno funciona após `7c499d5` e relatou oscilação no
botão de ajuste, solicitando bloquear novos cliques até concluir cada ação.
A interface agora mantém a intenção pendente durante os estados intermediários
do coordenador: bloqueia switch, seleção de saída, atualização e nova tentativa
no próprio clique; mostra spinner e mensagem de preparação/restauração sem
aguardar o timer. A parada e a recuperação aguardam o ACK assíncrono. O volume
continua disponível. O switch só recebe atualização visual quando seu valor muda.

A intenção do coordenador distingue preparação de cancelamento por outra ação,
para não prender o botão se uma abertura for cancelada enquanto o celular segue
conectado. Callbacks usam referências fracas aos widgets e a espera não bloqueia
a thread GTK. O método de nova tentativa retorna a confirmação da restauração.

TDD: apresentação inicialmente permitia alternar em Opening; o teste GTK falhou
no bloqueio imediato antes do polling. Após corrigir esses casos, outro teste
reproduziu controles presos após cancelamento externo. Todos passaram após a
correção. A cobertura GTK exercita cliques rápidos (inclusive eventos forçados
em switch desabilitado), consulta pausada por barreira, ausência de oscilação por
250 ms, execução de callback idle durante a espera, parada, erro/retry, cancelamento
externo e liberação da janela com consulta pendente. O teste antigo de controles
aguarda o botão ser habilitado antes de simular o próximo clique.

Validação final: **110 testes comuns aprovados, cinco ignorados por padrão**;
os dois testes GTK do retorno executados separadamente e aprovados; build offline
sem warnings, fmt e diff aprovados. Revisão independente sem achados importantes.
A main continua revertida; mudanças ficam na branch até validação do usuário.

## Reintegração aprovada

Após validação manual do retorno e aprovação do usuário, integração reaplicada em
`d443418` e merge das correções em `6bc450e`. A árvore integrada corresponde à
branch validada. Na main: **110 testes aprovados, zero falhas, cinco ignorados**;
build offline sem warnings, fmt e diff aprovados. Branch preservada, sem push.
