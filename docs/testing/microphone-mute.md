# Validação do mute

Branch `feat/microphone-mute`, base `e7668f2` (aparência adaptável ainda sem merge).

## Comportamento

Controle no topo fixo, ao lado do estado da conexão. Toggle com texto e destaque
quando ativo, acessível por teclado e com tooltip explicando o alcance do mute.
Somente o PCM enviado ao microfone virtual é zerado, depois da captura A/B e do
envio ao retorno. A conexão e os estados internos do DSP continuam ativos.
Entrada continua medida; saída apresenta zero enquanto o mute estiver ativo.
Trocar perfil preserva mute; reiniciar o aplicativo inicia desmutado.

A mudança entra no próximo bloco processado. Não remove áudio já entregue aos
buffers do PipeWire ou de outros programas. Gravações de teste e retorno no fone
continuam contendo a voz, inclusive durante o mute.

## TDD e revisão

- RED de áudio: PCM continuava não nulo após marcar mute. GREEN com zeramento
  depois das rotas locais. O teste usa DSP, fila de retorno e gravador reais,
  sem placa de som: alterna desmutado/mutado/mutado/desmutado e compara todas as
  amostras do retorno e da gravação original/tratada. Confere níveis, continuidade
  do processamento, preservação do mute ao trocar perfil e padrão ao iniciar.
- RED GTK: botão ausente no topo. GREEN após adicionar controle. A primeira posição
  em linha extra reduziu demais a área rolável; botão passou para a linha do status.
  Teste verifica toggle, texto, destaque, parâmetros e geração preservados, posição
  fora da rolagem, visibilidade e espaço útil em 420×400 e 1200×800.
- Revisão independente sem problemas críticos/importantes. Sugestão menor de
  comparar a gravação amostra por amostra incorporada ao teste.

## Verificação

Suíte comum: 119 aprovados, zero falhas, 13 ignorados por padrão.
Executados também os testes GTK `mute_control` e `fixed_monitoring_header`,
em processos separados, ambos aprovados. Build offline, fmt e diff aprovados.
Validação auditiva com celular e aplicação de chamada não executada neste lote.

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo test --offline --manifest-path app/Cargo.toml mute_control -- --ignored --test-threads=1
cargo test --offline --manifest-path app/Cargo.toml fixed_monitoring_header -- --ignored --test-threads=1
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```
