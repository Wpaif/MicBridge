# Validação da lateral fixa de conexão

Branch `feat/fixed-connection-sidebar`, base `1cd53e8`.

## Entrega

Conexão fora da rolagem, fixa na esquerda abaixo do status, com limite de 340 px.
Corpo e status centralizados num limite de 1800 px. Somente o estúdio direito rola;
suas colunas respondem à largura útil de um BreakpointBin, preservando os limites
1240/800 px e a proximidade entre teste A/B e retorno. QR inline continua no estúdio.

Janela com largura até 1180 px ou altura até 680 px recolhe a lateral e apresenta
botão de rede com nome acessível Conexão. O botão abre diálogo nativo com o mesmo
painel. Apenas o diálogo permite rolagem quando necessária para alcançar todos os
controles. Fechar/reabrir ou redimensionar para desktop não duplica o painel nem
perde sinais/estado. Medidores empilhados até 600 px permanecem independentes do
breakpoint interno do estúdio.

## TDD e revisão

- RED inicial: o painel de conexão tinha ScrolledWindow como ancestral.
- GREEN: posição estável após rolar o estúdio, abaixo do status e à esquerda;
  largura limitada; controles visíveis no modo fixo.
- Diálogo: teste em 420×400 e 1440×500, fechar/reabrir, alterar pareamento e
  redimensionar com diálogo aberto; painel retorna à mesma lateral com estado.
- O teste encontrou reentrada síncrona de `closed` durante `force_close` sobre
  RefCell emprestado. Substituído por WeakRef direto, sem empréstimo durante o sinal.
- Teste das colunas detectou lateral expandindo para 629 px pela largura natural
  dos filhos. Adicionado clamp externo de 340 px, também ocultado no modo compacto.
- Revisão independente sem problemas materiais. Sugestão de verificar o limite
  desktop incorporada: 1181×681 com impressão digital completa e subtítulo de
  pareamento em duas linhas; painel cabe por inteiro. Revisor confirmou a ligação
  do clamp e preservação de estado.

## Resultados

123 testes comuns aprovados; 16 ignorados por padrão. Cinco testes GTK executados
explicitamente em processos separados com `G_DEBUG=fatal-warnings`:
`connection_sidebar_stays`, `studio_panels_use`, `fixed_monitoring_header`,
`custom_audio_profiles_follow`, `mute_control`. Zero falhas/avisos nessas execuções.
Build offline sem warnings, fmt e diff aprovados. Sem alterações no áudio/rede.

Capturas reais: `/tmp/micbridge-studio-maximized.png` e
`/tmp/micbridge-fixed-connection.png`. Estado de conexão nos testes não vem de
celular real; não foi realizada validação auditiva neste lote.

```sh
cargo test --offline --manifest-path app/Cargo.toml
G_DEBUG=fatal-warnings cargo test --offline --manifest-path app/Cargo.toml connection_sidebar_stays -- --ignored --test-threads=1
G_DEBUG=fatal-warnings cargo test --offline --manifest-path app/Cargo.toml studio_panels_use -- --ignored --test-threads=1
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```
