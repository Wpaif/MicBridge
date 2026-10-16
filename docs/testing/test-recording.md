# Validação da gravação de teste A/B

Branch: `feat/test-recording`. Especificação e plano em `docs/superpowers`.

## TDD e revisão

Cada etapa foi executada com falhas observadas antes da implementação:

| Etapa | Evidência de vermelho → verde |
|---|---|
| Captura | 5 falhas iniciais: ausência de amostras, limite, IDs e desconexão; depois captura pareada e fila cheia verificadas |
| Integração DSP | 4 falhas iniciais: captura dos dois lados e finalização em mudanças de conexão/término do pump |
| WAV | 5 falhas iniciais: cabeçalho/PCM, escrita dos dois arquivos, caminhos iguais, dados inválidos e erros parciais |
| Reprodução | 4 falhas iniciais nos testes com processos reais; mais 1 falha na coordenação de nova gravação/encerramento |
| Apresentação/exportação | 4 falhas iniciais de estados e cancelamento; mais 1 falha de mensagem de resultado atrasado |
| Revisão independente | 6 testes de regressão falharam, reproduzindo quatro problemas de concorrência/exportação, e passaram após correção |

A revisão independente identificou e a implementação corrigiu:

- Finalização de nova captura antes de consumir seus blocos aceitos.
- Callback de conexão antiga interrompendo uma captura nova, e início desconectado antes da atualização da interface.
- Link simbólico pendente fazendo o arquivo tratado sobrescrever o original.
- Destino surgindo durante exportação sendo sobrescrito sem confirmação.

A captura finaliza somente após consumir os blocos aceitos; transições de conexão e
início da captura são serializadas fora do caminho de áudio. O gravador não espera
por mutex de buffers, disco ou consumidor no produtor. A exportação usa criação
exclusiva sem autorização de sobrescrita, rejeita links simbólicos/arquivos especiais
e verifica a identidade dos arquivos abertos antes de truncar.

## Comandos

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```

A suíte comum possui 50 testes aprovados e um teste gráfico separado, que também foi
executado e aprovado em uma sessão GTK:

```sh
cargo test --offline --manifest-path app/Cargo.toml \
  recording_ui::tests::panel_actions -- --ignored --test-threads=1
```

Esse teste aciona os botões reais de gravação/parada, processa PCM pelo DSP e verifica
as amostras e a sensibilidade dos controles. É separado para permitir a suíte comum
sem servidor gráfico. Os testes de reprodução usam processos controlados, inclusive
escrita bloqueada, troca de versão, falha e encerramento do filho.

As capturas visuais em 1400×900 e 420×780 foram inspecionadas. Um teste real de
`pw-cat` enviou 100 ms de silêncio à saída padrão do PipeWire e retornou código 0.

## Decisões e limites

- Usou-se a branch solicitada no checkout compartilhado, sem worktree adicional.
- Shared coordena parada da reprodução e início da captura; Recorder permanece independente.
- O início da gravação pode ser recusado se a conexão já não estiver ativa.
- Exportação requer arquivos regulares: links simbólicos não são destinos aceitos.
- Não houve comparação auditiva de fala recebida de um celular real, nem medição de
  latência total. A reprodução silenciosa prova abertura da saída, não qualidade audível.
- Não há compensação do atraso interno do RNNoise nesta versão.
- O número de amostras é limitado; a capacidade das Vecs e a fila acrescentam memória
  limitada ao tamanho aproximado de 11,52 MB dos dois buffers de amostras.
- Recolhimento de filhos foi testado em cancelamento/drop; o encerramento completo do
  processo usa também a proteção Linux de morte com o pai, sem teste independente de
  todos os modos possíveis de término do aplicativo.
- Corridas gerais preexistentes de negociação de rede não foram redesenhadas nesta branch.

Não houve sugestões menores adiadas pela revisão. Não foi iniciado o próximo recurso.
