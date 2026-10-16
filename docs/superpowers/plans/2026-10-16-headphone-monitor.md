# Retorno no fone — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans para execução direta, tarefa por tarefa, com revisão independente ao final. Usar os checkboxes para registrar progresso.

**Goal:** Ouvir a voz tratada no dispositivo escolhido, com volume independente, sem acrescentar espera ao áudio principal.

**Architecture:** Descoberta de dispositivos e reprodução ao vivo ficam em workers próprios. O pump envia blocos pequenos por fila limitada; um coordenador serializa a alternância entre retorno e A/B, aguarda o encerramento do filho anterior e rejeita operações antigas. Shared mantém a validação de conexão e os identificadores das operações.

**Tech Stack:** Rust 2024, GTK4/libadwaita, Tokio, serde_json, PipeWire (`pw-dump`, `pw-cat`) e bibliotecas já presentes; sem novas dependências obrigatórias.

**Spec:** `docs/superpowers/specs/2026-10-16-headphone-monitor-design.md`.

## Global Constraints

- Branch `feat/headphone-monitor`, derivada da `main` após o merge `8798d45`.
- Implementar com TDD, revisão independente e merge local somente após testes aprovados.
- Manter commits no padrão do projeto: tipo/escopo e descrição em português.
- Retorno desligado inicialmente; volume entre 0 e 100%, inicialmente 25%; rampa de 5 ms.
- Configurações permanecem na sessão; reiniciar o app não liga o retorno automaticamente.
- Cada entrada contém no máximo 480 amostras (10 ms); a fila comporta seis entradas, no máximo 60 ms de áudio aguardando consumo.
- O produtor não faz consultas a dispositivos, chamadas a processos, I/O de áudio nem espera por mutex de buffers/consumidor.
- Ativação exige conexão ativa e dispositivo selecionado ainda disponível.
- Trocar o destino desliga o retorno; não escolher outra saída automaticamente.
- Listagem em worker com timeout de 2 s; atualizar na abertura, mediante botão e a cada 2 s enquanto houver retorno ativo.
- Não exibir um número de latência como se incluísse celular, rede, Opus, RNNoise e hardware.
- Falhas do retorno não mudam o estado geral da conexão nem impedem gravações e chamadas.

## Review Focus

1. Uma ativação em validação de dispositivo seguida de desconexão ou outra escolha não pode reaparecer ligada.
2. A saída reaparece com o mesmo nome e outra identidade: interromper, nunca seguir o novo dispositivo silenciosamente.
3. Um pacote decodificado de 120 ms não cabe na fila de 60 ms: o retorno deve falhar de maneira explícita, preservando a saída principal.
4. Uma parada é substituída por outro comando antes de o worker lê-la: a confirmação ainda precisa provar que o filho antigo foi recolhido.
5. A interface é destruída durante listagem ou alternância A/B: workers devem poder terminar/cancelar sem manter a janela viva.

## Arquivos e contratos

- `app/src/monitor/devices.rs`: parsing/listagem de dispositivos e proteção contra consultas antigas.
- `app/src/monitor/gain.rs`: ganho suavizado exclusivo do retorno.
- `app/src/monitor.rs`: sessão, fila de PCM, status e worker de `pw-cat`.
- `app/src/local_audio.rs`: coordenação entre A/B e retorno, sem I/O nas callbacks GTK.
- `app/src/monitor_ui.rs`: painel GTK e apresentação dos estados.
- `app/src/recording/playback.rs`: confirmação assíncrona de parada/recolhimento do filho existente.
- `app/src/state.rs`, `server.rs`, `main.rs`, `recording_ui.rs`: integração aos fluxos já existentes.
- `README.md`, `DESIGN.md`, `docs/testing/headphone-monitor.md`: uso e validação.

Contratos entre tarefas:

- `OutputDevice { id: u32, serial: Option<String>, name: String, label: String }`; `same_instance(&self, other: &Self) -> bool` compara nome, ID e serial quando disponível. Sem serial válido, a listagem permanece possível, mas a abertura deve falhar localmente para impedir reutilização silenciosa do nome.
- `devices::parse(json: &str) -> Result<Vec<OutputDevice>, String>`; aceitar apenas `PipeWire:Interface:Node`, `Audio/Sink`, nome não vazio, ID válido; excluir `vdo_input` e nodes com prefixo interno `micbridge_monitor`.
- `devices::query() -> impl Future<Output = Result<Vec<OutputDevice>, String>>`: executar `pw-dump` com timeout, cancelar/recolher filho em erro ou timeout.
- `DeviceCatalog::new()`, `refresh() -> u64`, `snapshot() -> DeviceSnapshot`; snapshot contém query ID, lista, busy e erro. Apenas a consulta mais recente publica resultados.
- `MonitorId = u64`; `MonitorPhase { Disabled, Opening, Active, Failed }`.
- `MonitorStatus { id: MonitorId, phase: MonitorPhase, target: Option<OutputDevice>, generation: Option<u64>, volume_percent: f32, error: Option<String> }`.
- `Monitor::new() -> Monitor`, `status() -> MonitorStatus`, `set_volume(percent: f32)`, `active_id(generation: u64) -> Option<MonitorId>`, `push(id: MonitorId, generation: u64, samples: &[f32])`.
- Métodos internos de controle: `Monitor::open(id: MonitorId, generation: u64, target: OutputDevice, lease: Arc<AtomicBool>)` e `Monitor::stop_and_wait() -> impl Future<Output = Result<(), String>>`. O coordenador fornece IDs monotônicos e uma lease invalidada ao cancelar; somente a lease atual pode aceitar/publicar áudio.
- `GainRamp::new(percent: f32)`, `set_percent(percent: f32)`, `apply(samples: &mut [f32])`: linear em amplitude, 240 amostras para atingir novo alvo; alvo não muda se o valor solicitado for inválido; valores finitos são limitados a 0..100.
- `Playback::stop_and_wait() -> impl Future<Output = Result<(), String>>`: nova confirmação assíncrona de que o filho anterior já terminou; não inferir isso de `status.playing == false`.
- `LocalAudio::new(monitor: Monitor, playback: Playback) -> LocalAudio`: ator de comandos, sem referência forte a Shared ou widgets.
- `LocalAudio::request_monitor(generation: u64, target: OutputDevice, lease: Arc<AtomicBool>) -> MonitorId`, `request_playback(take: Arc<Take>, version: Version)`, `stop_monitor()`, `stop_playback()`, `shutdown()`; todas retornam imediatamente, invalidam comandos anteriores conforme o modo e fazem a espera pelos workers fora do chamador.
- Shared expõe `monitor: Monitor`, `devices: DeviceCatalog`, `local_audio: LocalAudio`; `start_monitor(target: OutputDevice) -> Result<MonitorId, String>`, `select_monitor_output(target: Option<OutputDevice>)`, `stop_monitor()`, `play_recording(take: Arc<Take>, version: Version)`, `stop_recording_playback()`.
- `monitor_ui::panel(shared: Arc<Shared>) -> gtk::Box` e `monitor::submit_processed(monitor: &Monitor, generation: u64, pcm: &[f32])`.

### Task 1: Descobrir e identificar saídas

**Files:** criar `monitor/devices.rs`; declarar `monitor`/`devices` no módulo novo e em `main.rs`.
**Interfaces:** produz OutputDevice, parse/query e DeviceCatalog acima; consumidor é o coordenador/UI das Tasks 4/5.

- [x] Confirmar branch/árvore limpa; executar `cargo test --offline --manifest-path app/Cargo.toml`. Esperado: 50 testes aprovados, teste GTK separado. Registrar BASE no ledger específico deste plano.
- [x] Escrever testes `parse_filters_sinks_and_preserves_distinct_identities`: fixture com hardware, saída virtual, fontes, `vdo_input`, `micbridge_monitor_*`, nomes amigáveis iguais e IDs diferentes; afirmar exatamente os nomes/identidades aceitos.
- [x] Escrever `parse_handles_missing_fields_and_invalid_json`: omitir name/classe/ID e não aceitar node incompleto; description ausente usa name; serial numérico/string é normalizado; JSON inválido ou raiz que não é array retorna erro.
- [x] Escrever testes de processo para sucesso, exit não zero e timeout de 2 s. Testes de parser não dependem de áudio; substituir somente o executável por um processo controlado nos testes do worker.
- [x] Escrever `stale_device_query_does_not_overwrite_newer_result` e `same_name_with_new_serial_is_another_device`; o worker de consulta antiga fica controladamente atrasado enquanto a consulta nova conclui.
- [x] Rodar `cargo test --offline --manifest-path app/Cargo.toml monitor::devices` e observar falhas; implementar parser e serviço com IDs monotônicos. Limitar saída de `pw-dump` a 8 MiB; excesso gera erro e encerra o processo.
- [x] Rodar testes focados e suíte completa, refatorar em verde; commit `feat(audio): lista saídas disponíveis para retorno no fone`.

### Task 2: Ganho independente e fila limitada

**Files:** criar `monitor/gain.rs` e núcleo em `monitor.rs`.
**Interfaces:** produz GainRamp, MonitorStatus e produtor Monitor; worker de processo completo é Task 3.

- [x] Escrever `volume_ramps_over_240_samples`: com entrada constante 1, volume de 25 para 100%, exigir primeiro ganho 0,253125, amostra 240 em 1 e as posteriores em 1. Usar tolerância 1e-6.
- [x] Escrever casos 0/25/100% estáveis, nova mudança no meio da rampa a partir do ganho corrente, NaN/infinito mantendo alvo anterior e percentuais finitos fora da faixa limitados a 0..100. Amostras não finitas viram zero no retorno.
- [x] Escrever `bounded_queue_stops_monitor_without_waiting_for_consumer`: consumidor pausado, seis blocos de 480 aceitos, sétimo interrompe com Failed; usar timeout do produtor em thread controlada para não deixar a suíte travada se houver regressão.
- [x] Escrever testes de sessão/geração antiga, desativação com fila pendente e ativação nova: nada antigo é aceito nem tocado. O volume inicial deve ser 25% a cada nova instância, mas mantido entre sessões na mesma execução.
- [x] Rodar `cargo test --offline --manifest-path app/Cargo.toml monitor` e observar falhas. Implementar fila de capacidade seis, tamanho por bloco limitado a 480, envio sem espera e validação de lease/IDs no produtor e consumidor.
- [x] Rodar suíte, refatorar em verde; commit `feat(audio): adiciona fila e volume independente do retorno`.

### Task 3: Worker de reprodução ao vivo e confirmação de parada

**Files:** `monitor.rs`, `recording/playback.rs`; usar `output::DieWithParent` existente.
**Interfaces:** implementa Monitor::open/stop_and_wait e Playback::stop_and_wait para a Task 4.

- [x] Escrever teste com processo controlado que recebe PCM f32 e argumentos reais: `--playback --raw --target <object.serial> --properties "node.dont-fallback=true node.dont-reconnect=true node.name=micbridge_monitor" --rate 48000 --channels 1 --format f32 --latency 20ms --media-role Music -`; verificar alvo e bytes após ganho.
- [x] Escrever testes de falha no spawn, escrita e exit, e cancelamento durante escrita bloqueada: processo terminado/recolhido antes de resolver stop_and_wait; falha não pode alterar a conexão nem reiniciar sozinha.
- [x] Escrever `coalesced_stop_acknowledges_child_reaping` para A/B e retorno: comandos Stop/Play substituídos rapidamente não podem perder a confirmação de encerramento. Associar confirmações ao número da operação; uma confirmação mais nova somente satisfaz a antiga se o filho anterior já tiver sido recolhido.
- [x] Escrever `stale_open_or_process_exit_cannot_publish_new_session_status`; invalidar lease durante abertura e escrita e verificar que não há áudio nem estado Active de sessão antiga.
- [x] Rodar testes novos e observar falhas. Implementar workers canceláveis, confirmações de parada reais e IDs de operação. Revalidar lease antes de aceitar áudio e publicar Active; um processo aberto tarde é encerrado sem enviar PCM.
- [x] Rodar suíte completa e testes anteriores A/B, refatorar em verde; commit `feat(audio): reproduz retorno com parada confirmada do fluxo`.

### Task 4: Coordenação de modos e integração ao pump

**Files:** criar `local_audio.rs`; modificar `state.rs`, `server.rs`, `recording_ui.rs`, `main.rs`.
**Interfaces:** produz LocalAudio e métodos Shared acima; consome Monitor, Playback e devices::query.

- [x] Escrever testes backend recusando ativação desconectada, sem escolha e com alvo removido/substituído. Usar a transação lifecycle existente para seleção de geração e emissão da lease; cancelar quando mudar conexão/seleção.
- [x] Escrever `stale_validation_after_disconnect_cannot_activate` e `new_selection_invalidates_pending_activation`: pausar consulta, mudar conexão/saída e liberar resposta; nenhuma sessão antiga fica ativa.
- [x] Escrever testes A/B→retorno e retorno→A/B com processos controlados: provar que o PID anterior desapareceu antes de o novo iniciar. Novos comandos substituem intenções antigas; desligar/cancelar durante a espera deixa ambos parados. Não aguardar futures segurando o mutex lifecycle.
- [x] Escrever `processed_audio_is_unchanged_with_monitoring`: DSP real + gravação simultânea, comparar PCM principal e buffers A/B byte a byte com retorno ligado/desligado. Verificar blocos de 480, 960, 5760 e 120 amostras.
- [x] Escrever caso de 5760 amostras com consumidor pausado: fila limitada leva a Failed local, saída principal e gravação continuam completas. Não aumentar a fila silenciosamente para caber o pacote.
- [x] Escrever testes de desconexão, reconexão e fim/erro do pump, retirada/troca de identidade do destino e shutdown: desativação descarta pendências, sem religamento automático. Iniciar uma gravação A/B preserva o retorno ativo.
- [x] Rodar os testes e observar falhas; implementar ator LocalAudio. Antes de abrir retorno, consultar saídas novamente e comparar a identidade selecionada; falhas de validação aparecem apenas no status local.
- [x] Integrar submit_processed após recording::process_block e antes de sink.write_mono; dividir com chunks(480), sem modificar pcm. Migrar as callbacks de reprodução A/B para os métodos Shared coordenados. Gravar teste continua parando A/B, mas não o retorno.
- [x] Rodar suíte completa, refatorar em verde; commit `feat(audio): coordena retorno e comparação A/B sem bloquear chamadas`.

### Task 5: Painel GTK e atualização de dispositivos

**Files:** criar `monitor_ui.rs`; modificar `main.rs`, `DESIGN.md` e `README.md`.
**Interfaces:** consome os contratos de catálogo, Shared e MonitorStatus; produz panel acima.

- [x] Escrever testes da apresentação: desligado inicialmente/25%, sem destino impede ativação, Opening indica espera, Active permite desligar, Failed apresenta erro; nenhuma falha local substitui o status da conexão.
- [x] Escrever testes preservando seleção por identidade após refresh, nomes amigáveis iguais com distinção visível e dispositivo removido marcado como indisponível sem seleção automática de outro.
- [x] Escrever teste de renovação inicial/manual e timer de 2 s somente enquanto ativo, sem consultas sobrepostas ilimitadas; resposta antiga não altera seleção ou erro mais recente.
- [x] Escrever teste gráfico separado, `#[ignore = "requer sessão gráfica GTK"]`: acionar seleção, volume, ligar/desligar com serviços/processos controlados; destruir janela durante consulta pendente e provar que ela não fica retida.
- [x] Rodar testes e observar falhas; implementar painel “Retorno no fone” junto ao teste de voz, lista com escolha explícita e Atualizar, volume e switch. Exibir “Use fones para evitar que o som volte ao microfone” e “O retorno tem atraso de monitoramento”.
- [x] Listagem, validação e espera de processos em workers; GTK apenas publica intenções e observa snapshots. Parar timer quando painel for destruído, usando referência fraca ao widget.
- [x] Rodar suíte e teste GTK explicitamente em sessão gráfica, inspecionar janela ampla e estreita; atualizar documentação. Commit `feat(ui): adiciona controles de retorno no fone`.

### Task 6: Revisão e merge

**Files:** criar `docs/testing/headphone-monitor.md`, atualizar checkboxes e ledger deste plano.

- [x] Rodar `cargo test --offline --manifest-path app/Cargo.toml`, `cargo build --offline --manifest-path app/Cargo.toml`, `cargo fmt --manifest-path app/Cargo.toml --check` e `git diff --check`. Esperado: zero falhas; teste gráfico explicitamente executado em separado.
- [x] Fazer uma revisão independente da branch completa, incluindo Review Focus. Corrigir achados importantes com testes de regressão RED→GREEN e suíte verde.
- [x] Verificar listagem e abertura de saída no PipeWire real disponível, sem afirmar validação auditiva de fala se não realizada. Registrar limitações, diferenças da especificação e resultados em `docs/testing/headphone-monitor.md`.
- [x] Registrar commits no padrão português, confirmar árvore limpa e fazer merge local `--no-ff` na `main`, já autorizado pelo usuário; reexecutar suíte após merge e preservar branch para organização.
- [x] Informar hash, resultados e o que requer teste manual no fone; não iniciar a funcionalidade seguinte neste lote.

Resultado: merge local `0dc7d6e` na `main`; suíte pós-merge com 78 aprovados e dois gráficos já executados separadamente. Branch preservada; sem push. Ver `docs/testing/headphone-monitor.md`.
