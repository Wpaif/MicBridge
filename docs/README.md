# Documentação do MicBridge

O MicBridge recebe áudio do navegador de um celular pela rede local, trata a voz
e a disponibiliza como microfone virtual no Linux com PipeWire.

Esta documentação descreve o código desta linha de desenvolvimento. Recursos
presentes aqui não representam, por si só, uma versão publicada ou já integrada
à `main`. Consulte o histórico Git antes de distribuir um build.

## Por onde começar

| Objetivo | Documento |
|---|---|
| Instalar, atualizar ou remover | [Instalação](installation.md) |
| Conectar o celular e configurar a voz | [Guia de uso](usage.md) |
| Entender pareamento, certificados e dados locais | [Segurança e dados](security.md) |
| Resolver conexão, áudio ou interface | [Solução de problemas](troubleshooting.md) |
| Entender os módulos e o caminho do áudio | [Arquitetura](architecture.md) |
| Compilar, testar e manter o repositório | [Desenvolvimento](development.md) |
| Alterar a interface mantendo suas convenções | [Design da interface](design.md) |
| Conferir as oito melhorias planejadas | [Plano de evolução](roadmap.md) |

## Como manter estes documentos

Atualize primeiro o guia correspondente ao comportamento alterado. Depois ajuste
o resumo e os links no [README principal](../README.md). O README apresenta o
projeto; os guias acima são a referência completa. Decisões de interface pertencem
ao guia de design e os procedimentos de teste ao guia de desenvolvimento.

Contagens de testes variam: use os comandos documentados em
[Desenvolvimento](development.md#testes) para verificar o estado atual.
