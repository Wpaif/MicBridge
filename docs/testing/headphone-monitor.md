# Validação do retorno no fone

Implementado na branch `feat/headphone-monitor`, sobre a gravação A/B já validada
manualmente pelo usuário. Execução sequencial com testes escritos antes das
implementações e revisão independente da branch completa.

## Cobertura e resultados

- Suíte: **78 aprovados**, dois testes gráficos ignorados por padrão.
- Testes gráficos executados separadamente: retorno e gravação A/B, ambos aprovados.
- Build offline, formatação e `git diff --check` aprovados.
- Parser: saídas físicas/virtuais, nomes repetidos, serial, JSON incompleto,
  timeout de 2 s, limite de 8 MiB e descarte de consultas antigas.
- Ganho: 0/25/100%, rampa de 240 amostras, mudança durante a rampa e valores inválidos.
- Fila: seis blocos de até 480 amostras, produtor sem espera, sobrecarga interrompendo
  apenas o retorno, gerações antigas descartadas.
- PCM principal e gravação A/B preservados byte a byte com DSP real para blocos de
  120, 480, 960 e 5760 amostras, inclusive saturação da fila de retorno.
- Processos controlados: argumentos, PCM com ganho, spawn/exit/escrita com falha,
  escrita bloqueada, confirmação de parada somente após recolher o filho,
  alternância A/B↔retorno, desconexão e ausência de reinício automático.
- GTK: seleção explícita, volume, ligar/desligar, atualização e liberação da janela
  durante consulta pendente. Capturas de 1600×950 e 520×900 inspecionadas.

Comandos principais:

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
cargo test --offline --manifest-path app/Cargo.toml monitor_ui::tests::panel_controls -- --ignored --test-threads=1
cargo test --offline --manifest-path app/Cargo.toml recording_ui::tests::panel_actions -- --ignored --test-threads=1
```

## Revisão e regressões

A revisão independente identificou dois problemas importantes e um menor. Todos
foram corrigidos, com regressões observadas em RED antes das correções:

1. O nome de um dispositivo pode ser reutilizado e o gerenciador de sessão pode
   escolher a saída padrão. O processo agora recebe `object.serial`, além de
   `node.dont-fallback=true` e `node.dont-reconnect=true`. Sem serial numérico,
   falha localmente. Testes verificam os argumentos e a recusa de identidade ausente;
   validação inicial e periódica cobrem retirada/substituição do dispositivo.
2. Um A/B cancelado podia iniciar entre a confirmação de parada e a publicação
   de reprodução. Uma lease e uma transação curta sincronizam publicação/cancelamento.
   Teste determinístico pausa nesse limite e exercita parada, shutdown e troca de modo,
   exigindo que a intenção antiga não publique outra operação.
3. A detecção de retirada não renovava o catálogo do painel. Agora solicita refresh,
   preservando a ordenação das consultas; regressão exige catálogo sem o dispositivo.

## Verificação real e decisões

`pw-dump` enumerou saídas interna e Bluetooth. Um `pw-cat` com o comando final abriu
a saída interna por serial e recebeu 100 ms de silêncio, encerrando com código 0.
O fluxo foi inspecionado via `pw-dump`: `target.object` correspondia ao serial
selecionado e ambas as propriedades de bloqueio estavam ativas.

O plano foi ajustado de alvo por nome para serial para cumprir a exigência de não
substituir silenciosamente a saída. Dispositivos sem serial podem ser listados,
mas não ativados. O construtor interno LocalAudio recebe também o catálogo para
revalidar o destino. Usou-se a branch no checkout compartilhado, sem worktree extra.

Os painéis locais ficam lado a lado em telas amplas e em coluna até 1100 px.
A captura ampla Wayland não produziu nó de renderização; a inspeção ampla foi
realizada com X11, e a estreita funcionou na sessão original.

Não houve escuta de fala recebida de um celular, medição de latência total ou
retirada física de hardware durante uma chamada. Os testes automatizados e a
abertura silenciosa não substituem essa validação auditiva. Para conferir no uso:
conecte o celular, escolha o fone, ative o retorno em 25%, fale, varie o volume e
alterne com A/B. Retire a saída e confirme interrupção sem troca para alto-falantes.
A fila limitada pode desligar o retorno sob sobrecarga; isso mantém o áudio principal
independente e exige reativação explícita. Nenhuma funcionalidade seguinte foi iniciada.
