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
