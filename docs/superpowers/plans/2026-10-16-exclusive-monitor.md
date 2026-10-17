# Retorno exclusivo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans para executar diretamente nesta sessão, tarefa por tarefa, com uma revisão independente da branch completa ao final. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Isolar temporariamente a saída escolhida para ouvir apenas o retorno tratado do celular, restaurando as conexões ao terminar.

**Architecture:** Um controlador de roteamento independente observa o grafo PipeWire, confirma os links do retorno e remove os concorrentes com registro persistente. O worker de PCM aguarda a preparação, mas continua independente das consultas ao grafo; sua confirmação de parada passa a incluir restauração. A coordenação A/B e os comandos GTK continuam publicando intenções sem aguardar processos na interface.

**Tech Stack:** Rust, Tokio, serde/serde_json, async-trait, libc, GTK4/libadwaita; ferramentas existentes pw-dump, pw-link e pw-cat. Sem novas dependências Cargo.

**Spec:** `docs/superpowers/specs/2026-10-16-exclusive-monitor-design.md`.

## Global Constraints

- Branch `fix/bluetooth-monitor`; preservar o teste Bluetooth em `app/src/monitor.rs`, atualmente sem commit.
- Trabalhar no checkout atual conforme a organização já adotada; não criar outro worktree.
- Somente links que terminam na saída explicitamente escolhida podem ser removidos. Outros destinos, fontes e microfone virtual permanecem fora da operação.
- Validar core, nodes, portas e links por identidade, incluindo serial; ID/nome isolados não bastam.
- Guardar registro durável antes de remover cada conexão. Confirmar o resultado pelo grafo, não apenas pelo exit code do comando.
- Volume local inicial 25%; fila de seis blocos de até 480 amostras, não bloquear nem alterar PCM principal/A/B.
- “Ativo” exige links de retorno corretos e ausência de concorrentes; não há troca automática de destino.
- Desligar, trocar destino, A/B, desconectar ou encerrar normalmente requer término do reprodutor e tentativa de restauração antes de outro modo.
- Restauração pendente bloqueia nova reprodução local e nova sessão exclusiva; o usuário pode tentar novamente.
- Diagnóstico geral permanece adiado. Não afirmar que exclusividade comprova a correção da ausência de voz.
- Commits com tipo/escopo em inglês e descrição em português; merge local após testes e revisão; preservar branch, sem push.

## Review Focus

1. WirePlumber recria links concorrentes: reconciliar e verificar novamente; falhar/rollback se o grafo não convergir.
2. Dispositivo/porta removido entre consulta e comando: revalidar imediatamente antes da mutação e confirmar depois; nunca planejar ação usando identidade substituída.
3. Processo termina após remoção ou antes de registrar confirmação: recuperação idempotente a partir do registro escrito antes da remoção.
4. Stop/Open são coalescidos durante restauração: ACK mais recente só satisfaz o anterior após cleanup e resultado da recuperação, sem iniciar A/B diante de erro pendente.
5. Segundo MicBridge/debug tenta controlar a sessão: lock não bloqueante impede dois donos de alterarem/restaurarem o mesmo registro.

## Arquivos e contratos

Criar `app/src/monitor/routing/{mod,graph,backend,journal,controller}.rs`. `graph.rs` interpreta o grafo e calcula ações puras; `backend.rs` executa ferramentas; `journal.rs` guarda operações pendentes; `controller.rs` gerencia exclusividade/restauração e observação; `mod.rs` expõe contratos.
Modificar `monitor.rs`/`worker.rs` para ciclo de vida e ACKs, `local_audio.rs`/`state.rs` para coordenação/recuperação, `main.rs` para encerramento, `monitor_ui.rs` para estados e retry. Atualizar README/DESIGN e criar `docs/testing/exclusive-monitor.md`.

Tipos compartilhados:

- `CoreIdentity { name: String, cookie: u32 }`, obtido de Core.info; cookie diferencia reinícios do daemon.
- `ObjectIdentity { id: u32, serial: u64 }`; serial ausente/inválido impede mutação desse objeto.
- `Endpoint { node: ObjectIdentity, node_name: String, port: ObjectIdentity, port_name: String, channel: String, direction: Direction }`, serializável.
- `SavedLink { core: CoreIdentity, output: Endpoint, input: Endpoint, passive: bool }`; preservar link.passive, recriar links persistentes sem copiar IDs/propriedades internas do link antigo.
- `Graph::parse(json: &str) -> Result<Graph, String>`; expõe core, nodes/portas/links e `target_matches(&OutputDevice) -> bool`.
- `Graph::return_links(target: &OutputDevice, stream_name: &str) -> Result<Vec<SavedLink>, String>`; nome único exato e nodes/portas identificados. FL→FL, FR→FR, MONO→MONO; aceitar mono para FL/FR somente quando a adaptação PipeWire expuser saídas FL/FR. Mapeamento ambíguo falha antes de remover concorrentes.
- `Graph::competitors(target: &OutputDevice, stream_name: &str) -> Result<Vec<SavedLink>, String>` e `Graph::validate_saved(&SavedLink) -> SavedState { Present, Missing, Unavailable }`.
- `#[async_trait] trait RoutingBackend: Send + Sync { async fn snapshot(&self) -> Result<Graph,String>; async fn connect(&self, link: &SavedLink) -> Result<(),String>; async fn disconnect(&self, link: &SavedLink) -> Result<(),String>; }`.
- `Journal::open(path: PathBuf) -> Result<Journal,String>` adquire lock de arquivo não bloqueante; `load() -> Result<Vec<SavedLink>,String>`, `save(entries: &[SavedLink]) -> Result<(),String>`, `clear() -> Result<(),String>`.
- `RestoreReport { restored: usize, existing: usize, unavailable: usize, pending: usize, error: Option<String> }`.
- `Routing::new(backend: Arc<dyn RoutingBackend>, journal_path: PathBuf) -> Routing` inicia worker próprio e recuperação inicial.
- `Routing::start(id: u64, target: OutputDevice, stream_name: String, lease: Arc<AtomicBool>) -> RouteSession`; `RouteSession::ready(&mut self) -> impl Future<Output=Result<(),String>>`, `failure(&mut self) -> impl Future<Output=String>`, `stop_and_restore(&self) -> impl Future<Output=RestoreReport>`.
- `Routing::recover() -> impl Future<Output=RestoreReport>`; operações de stop/restore são idempotentes. Nenhum future usa referência a widget/Shared.
- Injeção: `Monitor::with_services(executable: PathBuf, routing: Routing) -> Monitor`; testes de processo usam backend de grafo controlado, nunca o grafo real implicitamente. `Monitor::new()` usa backend real e registro em `state::data_dir()/monitor-routing.json`.
- `MonitorPhase` ganha `Restoring`; `MonitorStatus` ganha `restore_report: Option<RestoreReport>` e `restore_pending: bool`; manter erros de restauração até retry bem-sucedido.
- `Monitor::retry_restore()` apenas publica comando. `Monitor::stop_and_wait()` mantém assinatura e devolve Err quando a restauração permanecer pendente.
- ACK interno passa a `Settlement { operation: u64, error: Option<String> }`; limpeza e persistência precedem publicação.
- `LocalAudio::shutdown_and_wait() -> impl Future<Output=Result<(),String>>` publica shutdown e aguarda ACK; não segura mutex durante await.

Constantes e execução:

- Snapshot/controle de links: timeout de 2 s por processo, stdout máximo 8 MiB; filhos kill/wait em timeout, cancelamento e erro.
- Grafo: listener `pw-link -l -I -m` como gatilho, ignorando seu texto para decisões; consultar JSON novamente após evento. Coalescer eventos por 50 ms, auditoria a cada 250 ms, somente uma reconciliação em voo. EOF/erro do listener interrompe o modo com rollback, sem loop de reinício ocupado.
- Preparação: até 5 s para o fluxo/portas aparecerem; até quatro ciclos de mutação/confirmacão por reconciliação. Se não convergir, falhar e restaurar. Nenhuma promessa de isolamento instantâneo entre notificação e remoção de um novo link.
- Nome do retorno: `micbridge_monitor_<pid>_<id>_<nonce>` com nonce aleatório de 128 bits, gerado uma vez por sessão; passar o mesmo nome ao pw-cat e ao controlador.
- Registro v1 JSON, limite 8 MiB, até 4096 entradas únicas. Arquivo temporário 0600 no mesmo diretório, flush/sync_all, rename atômico e sync do diretório. Erro de escrita impede a remoção seguinte.
- Lock `monitor-routing.lock` com `libc::flock(LOCK_EX|LOCK_NB)` no worker; não truncar registro em falha, manter lock por toda recuperação/sessão. Não usar lock/I/O no produtor PCM.

## Task 1: Grafo e decisões de roteamento

**Files:** criar `monitor/routing/mod.rs`, `graph.rs`; declarar módulo em `monitor.rs`; preservar teste Bluetooth.
**Interfaces:** produz Graph, identidades, SavedLink e SavedState acima; consumidos pelas Tasks 2/3.

- [x] Escrever `only_selected_sink_competitors_are_removed`: fixture com dois aplicativos ligados ao fone, retorno FL/FR, outra saída e fonte; exigir apenas quatro links concorrentes do fone e preservar os demais.
- [x] Escrever `return_mapping_is_verified_before_unlink`: FL/FR correto, links ausentes, canal ambíguo e nome semelhante de outra sessão; não autorizar isolamento sem destino/canais válidos.
- [x] Escrever `reused_node_port_or_core_identity_is_unavailable`: trocar serial mantendo ID/nome, substituir porta e mudar cookie; nenhuma ação/restauração para o objeto substituto. Cobrir JSON truncado, objetos incompletos e propriedades numéricas/string.
- [x] Rodar `cargo test --offline --manifest-path app/Cargo.toml monitor::routing::graph`; observar RED comportamental com stubs mínimos.
- [x] Implementar parser/funções puras, preservação de passive e deduplicação por endpoints/identidade.
- [x] Rodar testes focados e suíte; commit `feat(audio): modela conexões do retorno exclusivo`.

## Task 2: Ferramentas e registro recuperável

**Files:** criar `routing/backend.rs`, `journal.rs`; testes com diretórios e executáveis temporários.
**Interfaces:** consome Graph/SavedLink; produz RoutingBackend real e Journal definidos acima.

- [ ] Escrever `commands_revalidate_and_confirm_endpoints`: processo controlado captura argumentos; snapshot substituído antes de agir impede connect/disconnect; exit 0 sem alteração do grafo não confirma sucesso. `pw-link -d` usa portas validadas, sem shell/interpolação.
- [ ] Escrever `backend_reaps_timeout_exit_and_oversized_snapshot`: spawn/exit, stdout >8 MiB, filho bloqueado e cancelamento; exigir erro e recolhimento. Conectar usa propriedades JSON/passive; nenhuma propriedade interna antiga é copiada.
- [ ] Escrever `journal_survives_interruption_before_or_after_unlink`: carregar intent gravada antes da remoção; link presente é idempotente; link ausente pode ser restaurado. Arquivo inválido ou >8 MiB não é apagado.
- [ ] Escrever `journal_save_failure_prevents_unlink_and_second_owner_is_rejected`: injetar falha de gravação e disputar lock; nenhum segundo controlador modifica registro/grafo. Cobrir deduplicação e limite de 4096.
- [ ] Rodar `cargo test --offline --manifest-path app/Cargo.toml monitor::routing` e observar RED; implementar backend e gravação durável com lock e limites.
- [ ] Rodar focados/suíte; commit `feat(audio): registra e restaura links do PipeWire`.

## Task 3: Controlador exclusivo e rollback

**Files:** criar `routing/controller.rs`; testes com backend mutável/barreiras, sem hardware.
**Interfaces:** consome Graph/Backend/Journal; produz Routing, RouteSession e RestoreReport.

- [ ] Escrever `activation_links_return_then_journals_unlinks_and_verifies`: registrar ordem; retorno conectado antes da primeira remoção e registro persistido antes dela; ready apenas após snapshot exclusivo.
- [ ] Escrever `new_competitor_is_removed_and_restored_once`: inserir link durante atividade e notificar; deduplicar reentradas e restaurar uma vez. Simular WirePlumber recriando continuamente: quatro tentativas geram falha/rollback.
- [ ] Escrever `partial_failure_rolls_back_and_retry_keeps_pending_record`: falhar após primeira remoção, falhar uma restauração, repetir; contagens e registro permanecem corretos, erro não desaparece silenciosamente.
- [ ] Escrever `cancelled_or_replaced_route_cannot_unlink_after_cleanup`: pausar snapshot/comando, invalidar lease e parar; aguardar mutação em voo e rollback antes do ACK, descartar resposta antiga e não publicar ready tardio.
- [ ] Escrever `recovery_respects_core_and_endpoint_identity`: abrir registro deixado após interrupção; restaurar válidos, contar presentes/indisponíveis e recusar reuso de ID/serial de outra sessão. Remover do registro apenas entradas cujo resultado foi confirmado; persistir falhas pendentes.
- [ ] Rodar focados em RED; implementar worker único, evento/auditoria/coalescência, preparação limitada, verificação pós-comando e restauração serializada. Revalidar antes de cada mutação; cancelamento não abandona remoção em voo sem rollback.
- [ ] Rodar suíte; commit `feat(audio): mantém saída exclusiva durante o ajuste`.

## Task 4: PCM, coordenação e encerramento

**Files:** modificar `monitor.rs`, `worker.rs`, `local_audio.rs`, `state.rs`, `main.rs`; atualizar fixtures em `audio_test.rs` e chamadas de construtores nos testes existentes.
**Interfaces:** consome Routing/RouteSession; produz novo ciclo Monitor e shutdown_and_wait definidos acima.

- [ ] Escrever `monitor_active_requires_route_ready_and_pcm_never_waits_for_graph`: barreira de snapshot mantém Opening; priming de até 480 amostras silenciosas permite negociar portas, sem aceitar voz antes de ready; atividade só depois da confirmação. Durante uso, consultas lentas não atrasam o consumidor PCM.
- [ ] Escrever `stop_ack_includes_reap_restore_and_error`: processo termina e links restauram antes do ACK; Stop/Open coalescidos preservam confirmação; falha pendente impede iniciar outro modo.
- [ ] Estender testes A/B↔retorno, troca de saída, desconexão e geração para pausar ativação/restauração; nova intenção aguarda cleanup e nunca reativa sessão antiga.
- [ ] Escrever `normal_quit_waits_for_restore_without_blocking_gtk`: fechamento da janela continua minimizando; evento Quit aguarda shutdown assíncrono antes de app.quit; caminho final após app.run também conclui cleanup antes de derrubar runtime.
- [ ] Rodar testes novos em RED; integrar nome único, controlador por sessão, novos estados, ACK com erro e retry. Recuperação inicial bloqueia abertura local até resultado; pending preserva erro/registro.
- [ ] Migrar também os testes existentes que constroem Monitor/Shared para serviços de roteamento injetados: backends controlados em memória e registro temporário. Nenhum teste comum/GTK deve executar pw-link real ou acessar o registro da sessão do usuário; apenas o teste Bluetooth explicitamente ignorado usa o backend real.
- [ ] Reexecutar regressão SHA-256 dos cinco perfis e PCM/A/B/retorno; suíte completa. Commit `fix(audio): coordena retorno exclusivo e restauração na parada`.

## Task 5: Interface, validação real, revisão e merge

**Files:** modificar `monitor_ui.rs`, README.md, DESIGN.md; criar `docs/testing/exclusive-monitor.md`.
**Interfaces:** consome MonitorStatus/Monitor::retry_restore; não inicia processos nas callbacks GTK.

- [ ] Escrever testes de apresentação para Disabled/Opening/Active/Restoring/Failed, pending e retry; manter seleção/volume e preservar erro de restauração mesmo desconectado.
- [ ] Escrever teste GTK ignorado por padrão: ativar, aguardar ready, desligar, observar restauração, falhar e repetir; janela liberada durante consulta pendente. Rodar RED antes da implementação.
- [ ] Implementar textos exatos da spec, aviso visível antes da ativação, botão “Tentar restaurar conexões” quando pending; controles de nova ativação ficam indisponíveis durante restauração/pending.
- [ ] Rodar suíte, build offline, fmt --check, git diff --check e cada teste gráfico em processo separado (`--ignored --test-threads=1`). Atualizar teste Bluetooth existente para identificar nome único e garantir cleanup mesmo em falha de asserção.
- [ ] Verificar grafo real antes/durante/depois com streams temporários: remover concorrente só da saída escolhida, confirmar links exclusivos e restauração. Capturar amostras de teste não silenciosas no monitor da saída; informar antecipadamente se o teste produzir som audível. Não afirmar escuta do celular sem validação do usuário.
- [ ] Registrar resultados TDD, limitações de captura/gráfico/hardware e comandos em docs/testing/exclusive-monitor.md; commit `feat(ui): apresenta ajuste exclusivo e recuperação de conexões`.
- [ ] Pedir uma revisão independente da branch inteira contra spec/plano; corrigir achados importantes com regressão RED→GREEN e suíte verde. Commit de correção no mesmo padrão se necessário.
- [ ] Confirmar árvore limpa; merge local `--no-ff` na main após aprovação prévia do fluxo, sem novo pedido de autorização; repetir suíte na main. Preservar branch, sem push; não iniciar diagnóstico/perfis neste lote.

## Self-review e fontes

Cobertura: UX/identificação/canais em Tasks 1/4/5; identidade/limites/registro/lock em 1/2/3; observação/rollback/recuperação em 3; PCM/ACKs/encerramento em 4; Bluetooth/GTK/revisão/merge em 5. Os cinco itens de Review Focus têm regressões nas Tasks 1–4. As tarefas consomem os tipos do contrato acima, sem novas dependências.

`pw-link` cria/remove links, aceita portas/IDs e fornece monitoramento; uso conferido no help instalado e no [manual oficial](https://docs.pipewire.org/page_man_pw-link_1.html). O snapshot real disponível contém Core.info.cookie e serial dos objetos; não usar apenas nomes na recuperação.
