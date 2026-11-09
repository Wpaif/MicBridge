---
name: interface-design
description: Use ao criar ou alterar a interface Quickshell/QML do MicBridge (app/ui) ou a página do celular em app/web.
---

# Design da interface do MicBridge

As convenções do projeto ficam em [Design da interface](../../../docs/design.md).
Leia esse guia antes de alterar layout, componentes, cores ou interação. Atualize
o guia no mesmo trabalho se uma decisão de interface mudar.

## Referências de trabalho

- [Arquitetura](../../../docs/architecture.md): responsabilidades dos módulos e o
  protocolo entre o daemon (`app/src/ipc.rs`) e a janela.
- [Desenvolvimento](../../../docs/development.md): como rodar o daemon com a
  interface do repositório (`MICBRIDGE_UI_DIR`) e os testes de QML.

Reutilize os componentes de `app/ui` (`Toggle`, `Slider`, `Card`, `Strip`, `Fader`,
`VMeter`...) e os tokens de cor do `Theme`. Faders e sliders não devem reagir à
roda do mouse. Todo controle precisa funcionar pelo teclado; atalhos novos entram
na folha de atalhos de `shell.qml`. A página do celular deve manter o fluxo de
captura simples.

Regras de áudio ficam no daemon, não no QML: a janela mostra o estado que recebe e
envia comandos. Captura com estado simulado não comprova funcionamento de rede ou
áudio; registre separadamente o que exigir celular, PipeWire ou fone.
