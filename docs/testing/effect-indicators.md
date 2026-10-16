# Validação dos indicadores dos efeitos

Branch: `feat/effect-indicators`, a partir de `e7b7ebf`. Mudança delimitada aprovada:
mostrar atuação do gate, redução do compressor e ajuste do ganho automático nos
controles existentes. Escopo de observação; sem alterar a cadeia de tratamento.

## Contratos

- Gate aberto quando aceita sinal, em espera durante a sustentação de pausas abaixo
  da histerese, atenuando após encerramento da espera. Desativado tem estado próprio.
- Compressor publica a maior redução aplicada nas 480 amostras do último frame;
  razão 1:1 produz 0 dB, efeito desativado não publica redução.
- Ganho automático publica o ganho ao final da rampa do frame, mantém o valor durante
  silêncio abaixo do detector e não publica ganho se desativado.
- Snapshot dos três efeitos em um AtomicU64, sem mutex/espera no DSP. Apenas a
  exibição é quantizada em 0,01 dB; a interface apresenta uma casa decimal.
- GTK usa o timer existente de 20 Hz e referências fracas aos controles. Um efeito
  desligado mostra “Desligado”; dados sem novos frames por 300 ms, desconexão e troca
  de geração não reutilizam medições antigas. Silêncio recebido conta como frame.
- Não são medidores de pico acumulado: alterações entre atualizações da tela podem
  não aparecer. O gate indica a decisão do detector, com a rampa de ganho preservada.

## TDD e verificações

Três testes DSP falharam com telemetria vazia antes da implementação: abertura/hold/
fechamento/bypass do gate; compressor de -12 dBFS com limiar -20 dB e razão 2:1
resultando em 4 dB; ganho automático positivo/negativo, congelamento e bypass.
Dois testes de apresentação/atividade falharam antes da implementação: estados,
valores com sinal, expiração, reconexão, nova geração e wrap do contador.
O teste GTK falhou com subtítulo vazio antes da implementação dos indicadores.

Uma referência SHA-256 do PCM foi obtida antes de modificar o processamento, usando
os cinco perfis, sinal senoidal, pausas e entradas NaN/infinito. O teste continua
exigindo os mesmos bits após a instrumentação. É uma regressão para a combinação
atual de arquitetura, dependências e implementação matemática, sem tolerância para
mudança no áudio. Atualizações futuras do DSP exigem avaliar essa referência.

A primeira execução verde do teste GTK atingiu uma falha nativa ao destruir uma
janela nunca apresentada. Backtrace com G_DEBUG=fatal-criticals/gdb identificou
`gtk_window_destroy`/superfície GDK ausente. O fixture foi corrigido para apresentar
e realizar a janela antes da destruição, seguindo os testes GTK existentes.

Comandos:

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
cargo test --offline --manifest-path app/Cargo.toml effect_ui::graphical_tests -- --ignored --test-threads=1
```

As verificações de interface usam DSP real e sessão GTK; não simulam uma chamada
com celular nem substituem a escuta manual. Revisão e integração são registradas
após a validação final.

## Revisão independente

Um achado importante foi reproduzido em RED: após troca de geração, um pacote
atrasado da conexão anterior incrementava o contador global e podia parecer
áudio novo. A telemetria agora fica em um slot por geração. O pump captura o
publicador antes de processar; processadores antigos só escrevem em seu próprio
slot, mesmo se forem criados tardiamente. Shared troca o slot na transação de
nova geração, e GTK usa seu contador local e valida a geração. A seleção do
publicador usa mutex uma vez na criação do pump; publicar cada frame usa apenas
atômicos. O construtor DSP genérico usado nos testes captura no primeiro frame.

A regressão pausa entre gerações, entrega frames antigos antes/depois dos novos
e exige ausência de atualização e preservação da medição atual. O PCM dos cinco
perfis permanece idêntico à referência após a correção.

## Resultado final

85 testes aprovados, zero falhas, três testes gráficos excluídos por padrão e
executados separadamente com sucesso (indicadores, retorno e A/B). Build offline,
formatação e diff aprovados. Leitura dos indicadores usa um único snapshot por
atualização. Layout amplo inspecionado em 1600×950; uma tentativa anterior de
captura em 1600×1400 não produziu nó de renderização e não conta como validação.
A interação/valores dos três controles foi verificada no teste GTK com DSP real.
Sem validação auditiva de uma chamada com celular nesta etapa.
