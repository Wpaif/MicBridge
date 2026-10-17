# Validação da aparência adaptável

Branch `feat/adaptive-appearance`, base `375534e`. Sem merge/push automático;
instalação existente e configurações globais do desktop preservadas.

## Entrega

Conexão e medidores ficam fora do ScrolledWindow, numa barra superior fixa.
Controles rolam abaixo. Até 600 px os medidores são empilhados; em 420×400 sobra
pelo menos 100 px para o conteúdo rolável com a fonte padrão. O corpo preserva a
organização em duas colunas em janelas largas e uma coluna até 1100 px.

Aparência abre num PreferencesDialog nativo/adaptável: Sistema, Claro, Escuro
e Personalizado; opacidade de fundo 60–100%; importar/exportar JSON v1,
prévia, aplicar, cancelar/fechar e restaurar padrão. Somente Aplicar persiste.
Acessibilidade: alto contraste desativa a paleta customizada/transparência;
rótulos dos medidores são widgets GTK com fonte relativa/cores nativas.

Modelo de temas limitado a 64 KiB, nome 80 caracteres, cores #RRGGBB, versão 1 e
contraste mínimo 4,5:1 do texto sobre fundo/superfície. Dados importados não executam
comandos nem carregam CSS externo. Exemplo em `app/data/themes/graphite.json`.
Leitura do arquivo é limitada, não bloqueante e aceita somente arquivos regulares.

Omarchy: caminhos atual XDG_STATE_HOME/omarchy/current/theme/colors.toml e legado
XDG_CONFIG_HOME/omarchy/current/theme/colors.toml; paletas semânticas e aliases
legados; recarga a cada 2 s em worker, sem reaplicar CSS quando não houve mudança.
Erro preserva última paleta válida, ou usa o sistema quando não há paleta válida.
Preferências são atualizadas atomicamente com mutex, preservando perfil e chaves
extras de settings.json. Cancelar restaura a preferência salva.

## TDD e revisão

- Modelos: 3 falhas RED e teste de rejeição, depois 4 GREEN; JSON claro/escuro,
  versão/cores/contraste/limites e paleta Omarchy semântica/legada.
- Persistência: RED antes da implementação, depois GREEN, cobrindo ambas
  as ordens de gravação, recuperação de valor inválido e preservação de chaves extras.
- GTK: RED para tema escuro forçado e medidores dentro da rolagem. GREEN com modo
  Sistema e barra fixa; capturas com conexão simulada revelaram largura 454 px em
  janela 420 px, corrigida pelo empilhamento dos medidores até 600 px.
- Diálogo: scaffold RED sem seletor; implementação moderna GREEN para prévia,
  cancelar sem salvar, aplicar/persistir e restaurar padrão como prévia cancelável.
- Teste de Omarchy acompanha arquivo modificado, preserva última paleta válida após
  TOML inválido e rejeita prévia inválida sem modificar estado.
- Pixel test: opacidade 60% rende alpha 153 no fundo; texto mantém alpha 1. Em alto
  contraste o fundo rende alpha 255.
- Revisão independente da branch inteira: dois Important/P2, nenhum Critical.
  Ambos reproduzidos RED e corrigidos: marcas internas dos medidores agora ficam
  em 20/40/60/80% (Grid com spans proporcionais, teste de coordenadas em 500/300 px);
  exportação resolve a cor nativa dos cards e compõe alfa sobre o fundo antes de
  gravar RGB. Teste compara a exportação/reimportação com pixels reais em claro e
  escuro. Não há segunda revisão obrigatória no fluxo de execução direta.
- Uma execução sob carga revelou uma corrida no teste existente de troca A/B↔retorno:
  esperava criação do PID antes de publicação Active. O teste agora aguarda Active
  antes de verificar que iniciar gravação preserva um retorno já ativo. Sem mudança
  no produto de áudio; regressão SHA-256 do PCM continua aprovada.

## Resultados

**118 testes comuns aprovados, zero falhas, 12 ignorados por padrão.** Os 11 testes
GTK foram executados explicitamente em processos separados; o teste Bluetooth
continua excluído porque este lote não altera áudio ou roteamento real. Build
offline sem warnings, fmt e diff aprovados.

Testes gráficos: fixed_monitoring_header, appearance_uses_system,
appearance_dialog_previews, omarchy_live_reload, background_transparency,
meter_ticks_match, exported_native_surface, exclusive_panel,
monitor_ui::tests::panel_controls, effect_ui::graphical_tests e
recording_ui::tests::panel_actions. Cada um usa `-- --ignored --test-threads=1`.
Layout foi repetido em Graphite, Claro e ADW_DEBUG_HIGH_CONTRAST=1; teste de alpha
repetido em alto contraste. Sem executar servidores WebRTC/PipeWire nos testes.

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```

Capturas reais da UI, com dados de demonstração de conexão/níveis:

```sh
MICBRIDGE_TEST_SNAPSHOTS=1 MICBRIDGE_TEST_APPEARANCE_SOURCE=custom GSK_RENDERER=cairo \
  cargo test --offline --manifest-path app/Cargo.toml fixed_monitoring_header \
  -- --ignored --test-threads=1
```

Arquivos `/tmp/micbridge-appearance-{custom,light}-{1200,420}[-top].png` foram
inspecionados. `-top` mostra início do conteúdo; sem sufixo, conteúdo rolado com
monitoramento fixo. A captura opcional do diálogo via WidgetPaintable não produziu
nó renderizável neste ambiente e foi retirada da fixture; os fluxos do diálogo
seguem validados pelo teste GTK. Não se trata de captura de voz real.

## Limites e compatibilidade

GTK ≥ 4.16 e libadwaita ≥ 1.6 explicitados no Cargo/README (variáveis semânticas de cor).
A leitura de cores nativas para exportação usa a API GTK de named colors com
allow(deprecated) localizado, pois não há substituta para resolver fundos.
Paletas explícitas são aplicadas apenas dentro de .mb-window com prioridade
local sobre gtk.css do usuário; modo Sistema continua usando as cores do usuário.

Validação no desktop disponível, claro/escuro/alto contraste e fixture Omarchy.
Instalações físicas de Omarchy, GNOME e KDE ainda requerem validação nesses
ambientes. Blur é responsabilidade do compositor e não foi configurado.

Ajuste visual adicional: 12 px entre a barra de título e o painel fixo e 12 px antes de Restaurar padrão. Suíte116 aprovada, build/fmt/diff aprovados; testes GTK de rolagem/tamanhos e de Aparência executados novamente e aprovados, com capturas Graphite atualizadas.

## Integração Omarchy automática no modo Sistema

Removida a escolha manual “Integrar com Omarchy”. Sistema detecta uma sessão
Hyprland com OMARCHY_PATH não vazio, ou desktop identificado como Omarchy.
GNOME/KDE/Plasma têm precedência e permanecem nativos; Hyprland sem marcador
Omarchy permanece nativo. Arquivos de paleta existentes, por si só, não selecionam
outro ambiente. O comportamento usa os marcadores publicados na
[configuração oficial da sessão](https://github.com/omacom/omarchy/blob/quattro/default/hypr/envs.lua).
A opção salva `omarchy` é desserializada como Sistema e serializada como `system`,
sem descartar opacidade/paleta customizada; a próxima gravação normal a canonicaliza.

TDD: migração, detecção de sessão, seletor e acompanhamento em Sistema reproduzidos
em RED antes da alteração. GREEN:118 testes comuns; três testes GTK relevantes
executados novamente (seletor/aplicar/cancelar, modo nativo e recarga Omarchy).
Recarga testa sessão simulada detectada, substituição da paleta, TOML inválido,
preferência Claro explícita e sessão nativa com arquivos Omarchy existentes.
Build offline sem warnings, fmt e diff aprovados. A instalação Omarchy real segue
uma validação manual; não foi alterada a configuração do desktop.
