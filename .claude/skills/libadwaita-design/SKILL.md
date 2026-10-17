---
name: libadwaita-design
description: Use ao criar ou alterar a interface GTK4/libadwaita do MicBridge ou a página do celular em app/web.
---

# Design da interface do MicBridge

As convenções do projeto ficam em [Design da interface](../../../docs/design.md).
Leia esse guia antes de alterar layout, componentes, cores ou interação. Atualize
o guia no mesmo trabalho se uma decisão de interface mudar.

## Referências de trabalho

- [Arquitetura](../../../docs/architecture.md): responsabilidades dos módulos.
- [Desenvolvimento](../../../docs/development.md): comandos de build, testes GTK
  em processos separados e capturas com dados de teste.

Reutilize os widgets existentes em `app/src/widgets.rs`, inclusive a fábrica de
deslizantes que protege contra rolagem acidental. Preserve estado ao reorganizar
painéis, acessibilidade, classes nativas da janela e o comportamento fixo do
status/conexão. A página do celular deve manter o fluxo de captura simples.

Use somente variáveis e comandos documentados no guia de desenvolvimento e
confirmados no código atual. Captura com status simulado não comprova funcionamento
de rede ou áudio; registre separadamente o que exigir celular, PipeWire ou fone.
