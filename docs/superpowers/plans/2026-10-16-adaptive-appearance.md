# Plano — aparência adaptável

Execução direta com TDD e uma revisão final independente. Branch feat/adaptive-appearance; checkout compartilhado conforme fluxo anterior. Base375534e. Sem merge/push antes da validação visual do usuário.

1. [x] Modelo de aparência/paleta, validação/importação/exportação e tradução Omarchy; testes RED→GREEN.
2. [x] Persistência com preservação mútua de perfil de áudio/aparência, e engine GTK com recarga assíncrona Omarchy; testes RED→GREEN.
3. [x] Topo fixo e CSS sem cores globais forçadas; teste GTK RED→GREEN de rolagem/tamanhos.
4. [x] Diálogo Aparência, prévia/importar/exportar/aplicar/cancelar/restaurar e feedback de erro; testes GTK RED→GREEN.
5. [x] Suíte, build, fmt, capturas reais, documentação, revisão final e correções. Commits convencionais em português. Manter branch para revisão visual.

Contratos: theme::Appearance (source,opacity,custom), theme::Palette, theme::ThemeController (preview,commit,cancel,current,status), settings::load_appearance/save_appearance. Modelo puro não importa GTK; engine aplica CSS só dentro de .mb-window e respeita alto contraste. Aparência e preset compartilham settings.json com atualização protegida/atômica. Imports limitados64KiB; Omarchy64KiB; sem I/O síncrono recorrente na thread GTK. `theme_ui` consome controlador; teste usa caminhos injetados e não toca preferências reais.
