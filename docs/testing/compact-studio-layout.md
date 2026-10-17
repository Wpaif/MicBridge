# Validação do agrupamento dos painéis

Branch `fix/compact-studio-layout`, base `1978724` (perfis personalizados).

## Alteração

FlowBox e pares de painéis com linhas homogêneas foram substituídos por colunas
verticais independentes. Cada painel mantém altura natural, com 18 px até o próximo.
Conteúdo e cabeçalho centralizados e limitados a 1440 px na janela maximizada.

- Acima de 1240 px: perfil/limpeza/timbre; dinâmica/volume; teste/retorno/conexão/QR.
- 801–1240 px: tratamento; teste/retorno/conexão/QR, limite total 1040 px.
- Até 800 px: uma coluna, limite 584 px incluindo margens.
- Até 600 px: medidores empilhados; nas demais larguras ficam lado a lado.

Os mesmos widgets são reposicionados, mantendo configurações, sinais e estado.
Foco é restaurado após a troca de colunas. Não há alteração no áudio, rede,
persistência, fluxo dos perfis ou funcionamento dos painéis.

## TDD e revisão

Teste gráfico RED mostrou Timbre na mesma linha de Limpeza, em vez de seguir sua
altura natural. GREEN mede os intervalos entre Limpeza/Timbre e Dinâmica/Volume,
largura das colunas (até 560 px), contagem, ausência de rolagem horizontal, área
útil em 420×400, centralização/limite ao maximizar e preservação dos parâmetros.

Revisão independente não encontrou problemas materiais. Sugestão de testar os
limites dos breakpoints incorporada: 1240/1241, 800/801 e 600/601 px, incluindo
retorno de uma coluna com medidores empilhados para horizontais e depois três.
Isso revelou aviso de largura mínima na transição original em 761 px. O aviso foi
reproduzido como falha com `G_DEBUG=fatal-warnings`; limite ajustado para 800 px,
repetição aprovada sem avisos.

Revisão de contexto: retorno foi movido da coluna de dinâmica para a coluna do
teste A/B. Regressão RED comprovou que os painéis estavam separados; GREEN verifica
que compartilham a coluna e são consecutivos, com 18 px de intervalo, em todas as
larguras e transições. Conexão e QR permanecem abaixo da área de testes.

## Verificação

123 testes comuns aprovados; 15 ignorados por padrão. Quatro testes GTK executados
em processos separados: `studio_panels_use`, `fixed_monitoring_header`,
`custom_audio_profiles_follow` e `mute_control`. Todos aprovados com avisos fatais.
Build offline sem warnings, fmt e diff aprovados.

Capturas reais da janela com conexão/níveis simulados para inspeção visual:

- `/tmp/micbridge-studio-maximized.png` (1920×1048, conteúdo centralizado).
- `/tmp/micbridge-studio-1440.png`, `/tmp/micbridge-studio-960.png`.
- `/tmp/micbridge-studio-420.png`.

```sh
cargo test --offline --manifest-path app/Cargo.toml
G_DEBUG=fatal-warnings cargo test --offline --manifest-path app/Cargo.toml studio_panels_use -- --ignored --test-threads=1
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```
