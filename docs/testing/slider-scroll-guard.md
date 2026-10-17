# Validação da proteção dos deslizantes contra rolagem acidental

Branch `fix/slider-scroll-guard`, base `6adea4c`.

## Correção

Fábrica compartilhada `widgets::slider` usada pelos seis deslizantes: gate,
limiar/razão do compressor, ganho manual, volume do retorno e opacidade.
Desativa apenas o EventControllerScroll nativo do GtkRange, por propagation phase
None. O evento segue para o ScrolledWindow; não há interceptação que consuma a
rolagem do painel. Arraste, teclado, foco, precisão e alteração programática são
preservados. Nenhuma alteração no DSP, rede ou persistência.

## TDD e entrada real

Teste GTK opt-in usa eventos XTest reais sobre uma janela de teste com um
deslizante focado dentro de ScrolledWindow. RED: três eventos de roda mudaram o
valor de 50 para 20. GREEN: valor permanece 50, ajuste vertical do painel avança,
seta direita muda para 51 e arraste continua aumentando o valor para acima de 60.

A sessão Xwayland rootless do desktop não entregava os eventos de ponteiro
simulados. O runner inicia um Xwayland rootful isolado com display escolhido
automaticamente; o teste processa o loop GTK enquanto envia a entrada. O servidor
é encerrado em finally, inclusive em falhas. O cursor do desktop não é usado.
Esse ambiente emite avisos não fatais do xkbcomp sobre seu mapa de teclado; GTK é
executado com `G_DEBUG=fatal-warnings` e não emitiu avisos na execução aprovada.

Python usa apenas ctypes da biblioteca padrão, libX11/libXtst já existentes.
Requisitos são opcionais para este teste gráfico; não foram adicionadas
dependências de aplicação ou linkagem X11/XTest aos testes comuns. O acesso ao
XID GTK é resolvido dinamicamente apenas no teste X11.

## Resultados

123 testes comuns aprovados, zero falhas; 17 ignorados por padrão. Três testes GTK
executados explicitamente: entrada real em sessão isolada, preferências de
aparência e controles do retorno. Todos aprovados. Build offline sem warnings,
fmt e diff aprovados. Revisão independente sem problemas materiais.

```sh
cargo test --offline --manifest-path app/Cargo.toml
# Wayland: sessão X11 isolada, sem instalar pacotes Python.
python3 app/tests/run_slider_scroll.py
# Alternativa em uma sessão X11 normal:
GDK_BACKEND=x11 cargo test --offline --manifest-path app/Cargo.toml slider_wheel_scrolls -- --ignored --test-threads=1
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```
