# Gravação de teste A/B — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (execução direta recomendada) ou superpowers:subagent-driven-development para implementar tarefa por tarefa. Usar os checkboxes para registrar progresso.

**Goal:** Capturar, comparar e exportar até 30 segundos de áudio original e tratado sem bloquear o áudio ao vivo.

**Architecture:** Um produtor envia pares de blocos por fila limitada a um consumidor dedicado. Um controlador mantém identificadores e estado da captura; módulos separados cuidam de WAV, reprodução e painel GTK. Gravação e reprodução não alteram o PCM entregue ao microfone virtual.

**Tech Stack:** Rust 2024, GTK4/libadwaita, Tokio, async-channel e PipeWire já usados no projeto; sem novas dependências obrigatórias.

**Spec:** `docs/superpowers/specs/2026-10-16-test-recording-design.md`.

## Global Constraints

- Branch: `feat/test-recording`; implementar uma funcionalidade por vez.
- Cada recurso terá sua própria branch, testes pelo ciclo vermelho–verde–refatoração, revisão e merge na `main` apenas depois da validação.
- Captura explícita, limitada a 30 segundos a 48 kHz mono.
- Buffers limitados a 1.440.000 amostras por versão, aproximadamente 11,52 MB no total em f32, além da fila.
- O produtor não espera por disco, reprodução ou pelo consumidor.
- Os dois buffers mantêm correspondência exata de amostras; não compensar o atraso interno do RNNoise.
- Sem amostras, reprodução e exportação ficam indisponíveis.
- Nenhum arquivo é salvo automaticamente. Sobrescrita exige confirmação no seletor.
- Uma desconexão finaliza a captura parcial; reconectar não continua nem mistura outra sessão no mesmo teste.
- As alterações de DSP e interface existentes na árvore de trabalho devem ser preservadas e consolidadas separadamente da implementação deste recurso.

## Review Focus

1. Parar com blocos já aceitos na fila deve preservar o prefixo completo, sem aceitar blocos posteriores.
2. Uma falha tardia de reprodução anterior não deve sobrescrever o estado de uma reprodução nova.
3. O segundo WAV já existente exige confirmação antes de gravar o primeiro; erro no segundo informa exportação parcial.
4. Desconexão sem novo pacote deve finalizar a captura mesmo com `track.poll()` pendente.
5. Encerrar o aplicativo durante escrita no stdin do reprodutor deve matar e recolher o processo filho.

## Arquivos e interfaces

- `app/src/recording.rs`: controlador, fila, buffers e snapshots.
- `app/src/recording/wav.rs`: codificação WAV e exportação.
- `app/src/recording/playback.rs`: reprodução exclusiva, ciclo de vida do filho.
- `app/src/recording_ui.rs`: painel, seletor de destino e apresentação de erros.
- `app/src/state.rs`: serviço de gravação compartilhado e vínculo com gerações da conexão.
- `app/src/server.rs`: captura antes/depois do DSP e finalização por desconexão.
- `app/src/main.rs`: declaração de módulos, painel e encerramento.
- `README.md` e `DESIGN.md`: uso e decisões da interface.

Contratos públicos entre tarefas:

- `RecordingId = u64`, `Generation = u64`.
- `enum TakeState { Empty, Recording, Ready, Incomplete }`.
- `enum Version { Original, Treated }`.
- `Take { id: RecordingId, original: Vec<f32>, treated: Vec<f32> }`.
- `RecordingStatus { id: RecordingId, state: TakeState, samples: usize, message: Option<String> }`.
- `Recorder::new() -> Recorder`: inicia consumidor dedicado; clones compartilham serviço.
- `Recorder::start(generation: Generation) -> RecordingId`: substitui teste anterior e solicita parada da reprodução vinculada antes de capturar.
- `Recorder::active_id(generation: Generation) -> Option<RecordingId>`: consulta atômica usada antes do DSP.
- `Recorder::push(id: RecordingId, generation: Generation, original: &[f32], treated: &[f32])`: tenta envio sem esperar; não aceita comprimentos diferentes.
- `Recorder::stop(id: RecordingId)` e `Recorder::disconnect(generation: Generation)`: selam o prefixo de blocos aceitos; finalização é idempotente.
- `Recorder::status() -> RecordingStatus`: consulta leve; não copia buffers completos.
- `Recorder::take(id: RecordingId) -> Option<std::sync::Arc<Take>>`: somente teste finalizado com amostras; snapshot imutável.
- `wav::encode(samples: &[f32]) -> std::io::Result<Vec<u8>>`.
- `wav::export(take: &Take, original_path: &std::path::Path, treated_path: &std::path::Path) -> Result<(), ExportError>`: `ExportError` informa quais arquivos foram concluídos.
- `Playback::new() -> Playback`, `Playback::play(take: std::sync::Arc<Take>, version: Version)`, `Playback::stop()`, `Playback::status() -> PlaybackStatus`; status inclui captura, versão e erro atuais.
- `recording_ui::panel(shared: std::sync::Arc<Shared>, parent: &adw::ApplicationWindow) -> gtk::Box`.

### Task 0: Consolidar base anterior separadamente

**Files:** alterações existentes de DSP, interface e `AUDIO_REVIEW.md`; sem alterações de implementação da gravação.

- [ ] Registrar hashes e diff antes de mover branches; preservar o commit `285a098` da especificação.
- [ ] Consolidar a correção do gate em `fix/gate-hold`, incluindo seus três testes e análise; executar `cargo test --offline --manifest-path app/Cargo.toml` e revisar antes de integrar à `main`.
- [ ] Consolidar a interface aprovada em `feat/studio-layout`; executar build, suíte, formatação e revisar antes de integrar à `main`.
- [ ] Atualizar `feat/test-recording` com a `main` e confirmar árvore limpa; não usar stash ou reset destrutivo para descartar trabalho.

### Task 1: Ciclo de vida e fila de captura

**Files:** criar `app/src/recording.rs`; declarar módulo em `main.rs`.
**Interfaces:** produz `Recorder`, IDs, `Take`, `RecordingStatus`, `TakeState` e `Version` definidos acima.

- [ ] Escrever testes com pares `[0.1, 0.2]` / `[0.3, 0.4]`, mais um segundo bloco; afirmar igualdade e ordem exatas dos dois buffers e duração por amostras.
- [ ] Testar 1.439.999 amostras seguidas de bloco de 480: ambos terminam exatamente em 1.440.000, estado Ready, sem amostras extras.
- [ ] Testar parar vazio, parar duas vezes, parar com blocos aceitos pendentes, iniciar novamente e rejeitar blocos com ID ou geração antigos.
- [ ] Testar fila de capacidade 1 com consumidor controlado: segundo envio retorna sem aguardar consumidor; teste finaliza Incomplete e preserva pares completos. Configuração de capacidade para testes fica em construtor privado do módulo, não na API de produto.
- [ ] Testar comprimentos diferentes: encerrar Incomplete com mensagem; nunca publicar buffers desalinhados.
- [ ] Executar `cargo test --offline --manifest-path app/Cargo.toml recording::tests` e observar falhas esperadas antes da implementação. APIs ausentes podem causar falha inicial de compilação; depois de criar assinaturas mínimas, observar falhas nas asserções antes de escrever comportamento.
- [ ] Implementar consumidor dedicado, fila de 64 pares com no máximo 5760 amostras por bloco e IDs monotônicos. Usar `try_send` no produtor; coordenação de parada preserva todos os blocos aceitos e rejeita blocos posteriores. Não bloquear o produtor com mutex de buffers ou operações de controle.
- [ ] Reexecutar os testes, refatorar com tudo verde e registrar commit `feat: add bounded A/B recording capture`.

### Task 2: Captura nos limites do DSP e desconexões

**Files:** `recording.rs`, `state.rs`, `server.rs`.
**Interfaces:** consome Recorder; expõe `recording::process_block(processor: &mut crate::dsp::Processor, pcm: &mut [f32], params: &crate::dsp::Params, recorder: &Recorder, generation: Generation)`.

- [ ] Escrever teste com DSP real e ganho manual conhecido, sem outros efeitos: original preservado, tratado igual ao PCM final e diferente do original; ambos com mesmo comprimento.
- [ ] Comparar o PCM entregue com captura desligada e ligada: resultados idênticos. Testar também bloco de 960 amostras e resto menor que FRAME, preservando o comportamento atual de emit.
- [ ] Testar início de captura durante processamento: nunca juntar original de uma captura com tratado de outra; ID deve ser obtido antes da cópia original.
- [ ] Testar desconexão sem pacote subsequente, erro do decoder, término do track e geração substituída: finaliza apenas a geração correspondente.
- [ ] Executar testes e observar falhas; implementar `process_block`, serviço em Shared, finalização no ciclo de vida da conexão e guarda de término do pump. Integrar ao `emit` antes de `sink.write_mono`.
- [ ] Reexecutar testes e suíte completa; commit `feat: capture original and processed audio in pipeline`.

### Task 3: WAV e exportação

**Files:** `app/src/recording/wav.rs`.
**Interfaces:** consome Take; produz encode, export e ExportError definidos acima.

- [ ] Escrever teste que lê bytes por offsets independentes: RIFF/WAVE, PCM 1, mono 1, 48000 Hz, 96000 bytes/s, alinhamento 2, 16 bits e tamanhos exatos.
- [ ] Para `[-2.0, -1.0, 0.0, 1.0, 2.0, NaN, +inf, -inf]`, decodificar i16 little-endian e exigir `[-32768, -32768, 0, 32767, 32767, 0, 0, 0]`.
- [ ] Testar dois arquivos distintos, caminhos iguais rejeitados antes de escrever, primeiro destino inválido e falha apenas no segundo com erro de exportação parcial. Usar diretórios temporários sem mudar variáveis globais do ambiente.
- [ ] Executar testes, observar falhas, implementar encode/export; escrita ocorre em worker, nunca na thread GTK ou pump.
- [ ] Reexecutar testes, suíte e commit `feat: export A/B test as paired WAV files`.

### Task 4: Reprodução exclusiva e cancelamento

**Files:** `app/src/recording/playback.rs`.
**Interfaces:** consome Take e Version; produz Playback e PlaybackStatus definidos acima.

- [ ] Escrever testes de troca original/tratado, parada explícita, nova captura, falha no spawn, filho com saída não zero e erro de escrita no stdin.
- [ ] Testar que uma conclusão antiga não altera o status de uma operação nova.
- [ ] Testar encerrar durante escrita bloqueada: filho encerrado e recolhido. Usar processos de teste controlados e um timeout de segurança; não exigir dispositivo de som nos testes automatizados.
- [ ] Executar testes e observar falhas; implementar worker assíncrono com cancelamento, IDs de operação, kill_on_drop e espera pelo encerramento do filho.
- [ ] Usar `pw-cat --playback --raw --rate 48000 --channels 1 --format f32 --media-role Music -`, sem target do microfone virtual; verificar argumentos e PCM enviado nos testes do limite com processo.
- [ ] Ligar início da nova captura e shutdown à parada da reprodução; reexecutar suíte e commit `feat: play original or treated recording exclusively`.

### Task 5: Painel GTK e fluxo de exportação

**Files:** `recording_ui.rs`, `main.rs`, `README.md`, `DESIGN.md`.
**Interfaces:** consome Recorder, Playback e WAV; produz panel definido acima. Shared expõe `pub recorder: Recorder`, `pub playback: Playback` e `pub fn generation(&self) -> u64`.

- [ ] Escrever testes da apresentação derivada do estado: Empty desabilita reprodução/exportação; Recording habilita parar; Ready habilita A/B/exportar; Incomplete mostra aviso preservando trecho disponível.
- [ ] Escrever testes do fluxo de exportação: cancelamento não chama escrita; ambos os destinos existentes são confirmados antes de qualquer escrita; erro do segundo mantém mensagem de exportação parcial. Usar decisões puras do controlador e arquivos reais, sem testar apenas chamadas de mocks.
- [ ] Executar testes e observar falhas; implementar painel “Teste de voz” com botões Iniciar, Parar, Original, Tratado, Parar reprodução e Exportar. Mostrar duração `samples / 48000`, limite 30 s e aviso para usar fones.
- [ ] Usar `gtk::FileDialog` para escolher o arquivo original; derivar o segundo com sufixo `-tratado.wav`. Se qualquer destino existir, mostrar confirmação explícita antes de exportar. Cancelar em qualquer etapa não grava.
- [ ] Usar snapshot imutável da captura escolhida no momento de exportar; resultados atrasados nunca substituem mensagens de captura/reprodução mais recentes. I/O em worker, atualização de widgets na thread GTK.
- [ ] Integrar painel abaixo dos perfis; atualizar documentação e capturar interface pequena/ampla. Não iniciar serviços de rede reais nos testes unitários de apresentação.
- [ ] Reexecutar testes e commit `feat(ui): add voice test recording and A/B controls`.

### Task 6: Revisão, validação e merge

- [ ] Rodar `cargo test --offline --manifest-path app/Cargo.toml`, `cargo build --offline --manifest-path app/Cargo.toml`, `cargo fmt --manifest-path app/Cargo.toml --check` e `git diff --check`; todos devem retornar zero.
- [ ] Revisar a branch inteira contra os critérios da especificação, principalmente fila cheia, paradas concorrentes, sobrescrita e filhos de reprodução.
- [ ] Fazer verificação disponível no PipeWire de reprodução e exportação; registrar separadamente o que não puder ser ouvido/validado com celular real.
- [ ] Com árvore limpa e verificações aprovadas, merge de `feat/test-recording` na `main` com commit de merge; executar novamente a suíte na `main`.
- [ ] Reportar hashes, resultados TDD e limitações; não iniciar automaticamente o próximo recurso nesta etapa.
