<div align="center">
  <img src="app/data/dev.wilian.MicBridge.svg" width="128" alt="MicBridge"/>

  <h1>MicBridge</h1>

  <p><strong>Use o celular como microfone do PC, com tratamento de voz em tempo real.</strong></p>

  <p>
  <a href="https://github.com/Wpaif/MicBridge/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Wpaif/MicBridge/ci.yml?style=flat-square&labelColor=0d1226&label=CI&logo=githubactions&logoColor=white" alt="CI"/></a>
  <img src="https://img.shields.io/badge/plataforma-Linux-f472b6?style=flat-square&labelColor=0d1226&logo=linux&logoColor=white" alt="plataforma Linux"/>
  <img src="https://img.shields.io/badge/Rust-2024-a78bfa?style=flat-square&labelColor=0d1226&logo=rust&logoColor=white" alt="Rust 2024"/>
  <img src="https://img.shields.io/badge/interface-Quickshell-818cf8?style=flat-square&labelColor=0d1226&logo=qt&logoColor=white" alt="interface Quickshell"/>
  <img src="https://img.shields.io/badge/%C3%A1udio-PipeWire-38bdf8?style=flat-square&labelColor=0d1226&logo=pipewire&logoColor=white" alt="áudio PipeWire"/>
  <img src="https://img.shields.io/badge/WebRTC-Opus-22d3ee?style=flat-square&labelColor=0d1226&logo=webrtc&logoColor=white" alt="WebRTC Opus"/>
  </p>
</div>

Use o celular como microfone do PC pela rede local, com tratamento de voz em
tempo real. O áudio sai do navegador via WebRTC/Opus e chega ao **MicBridge**,
uma entrada virtual do PipeWire que pode ser selecionada no Discord, OBS e outros
programas. Não é necessário instalar um app no celular.

Motor em Rust e interface QML aberta pelo [Quickshell](https://quickshell.org),
com as cores do tema Omarchy, uma mesa de som para os efeitos e atalhos de teclado.

## Recursos

- Perfis Natural, Chamada, Podcast, Streaming e Original, além de perfis locais
  personalizados com salvamento explícito.
- RNNoise, filtro de graves, gate, de-esser, presença, equalizador de 3 bandas,
  compressor, volume automático e limitador, numa mesa de som com faders, LEDs e
  medidores de atuação.
- Redução de ruído que preserva sílabas fracas, com fader de força para trocar
  um pouco de ruído por mais fala.
- Atalhos do equalizador: Plano, Clarear, Encorpar e Secar.
- Calibração automática: mede o silêncio e a sua voz e sugere limiar do gate,
  redução de ruído e ganho.
- Aviso de saturação (**CLIP**) na entrada e na saída, com dica do que fazer.
- Teste de voz de até 30 segundos, comparação original/tratado e exportação WAV.
- Retorno no fone com saída escolhida, volume próprio e ajuste exclusivo que
  restaura os links de áudio ao terminar.
- Mute da saída virtual, preservando retorno e gravação de teste.
- Fonte extra: o som de um app (como uma aba do navegador) junto com a voz.
- Pareamento por QR, HTTPS com CA local, reconexão e anúncio mDNS opcional.
  Um celular já pareado pode ler o QR novamente só para abrir a interface, sem
  reabrir o pareamento. Vários celulares podem ser pareados, mas só um fica
  conectado por vez.
- Cores que acompanham o tema do Omarchy automaticamente.
- Faders protegidos contra alterações acidentais durante a rolagem.

A redução de ruído lida bem com ruído constante (ventilador, ar-condicionado,
zumbido elétrico), menos com cliques de teclado, e não separa vozes de outras
pessoas ao fundo.

Os recursos acima descrevem esta linha de desenvolvimento, não uma release
publicada. Consulte o [plano de evolução](docs/roadmap.md) para itens pendentes.

## Instalação

Requer Linux com PipeWire, Rust/Cargo, libopus, [Quickshell](https://quickshell.org)
e ferramentas de compilação/áudio. A instalação também usa `rsvg-convert`.

```sh
git clone https://github.com/Wpaif/MicBridge.git
cd MicBridge
./scripts/install.sh
```

O script compila em release e instala para o usuário atual. Para requisitos,
atualização, reinstalação limpa e remoção, veja [Instalação](docs/installation.md).

## Começar a usar

1. Abra o MicBridge no PC e coloque o celular na mesma rede.
2. Habilite **Aceitar novo celular**, escaneie o QR e autorize o microfone no navegador.
3. Toque em **Iniciar microfone** no celular. Nas próximas vezes, ler o QR de novo
   abre a interface; **Aceitar novo celular** só é preciso para um celular novo.
4. Escolha **MicBridge** no programa de chamada ou gravação.
5. Use **Calibrar** (seção Voz) para ajustar ao seu ambiente, refine o perfil na
   mesa de efeitos e compare com A/B ou retorno no fone.

O navegador precisa aceitar o HTTPS. Consulte [Certificados](docs/security.md#certificados).
O retorno exclusivo desconecta temporariamente outros aplicativos da saída
selecionada; a parada tenta restaurá-los. Fechar a janela só a esconde; use
**Sair** na bandeja para encerrar o app.

Instruções completas de perfis, efeitos, calibração, mute, testes, retorno e fonte
extra estão no
[Guia de uso](docs/usage.md).

## Documentação

| Assunto | Referência |
|---|---|
| Todos os guias | [Índice](docs/README.md) |
| Erros de conexão, cortes e retorno | [Solução de problemas](docs/troubleshooting.md) |
| Pareamento, TLS e dados locais | [Segurança e dados](docs/security.md) |
| Módulos e caminho do áudio | [Arquitetura](docs/architecture.md) |
| Build, testes e arquivos locais | [Desenvolvimento](docs/development.md) |
| Convenções da interface | [Design](docs/design.md) |
| Melhorias, próximos passos e avaliações | [Evolução](docs/roadmap.md) |

## Desenvolvimento

```sh
cargo build --offline --manifest-path app/Cargo.toml
cargo test --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
cargo clippy --offline --manifest-path app/Cargo.toml --all-targets -- -D warnings
```

O modo offline exige dependências previamente disponíveis. O CI recusa
formatação fora do padrão e qualquer aviso do clippy. Os testes de qualidade do
DSP misturam uma voz sintética a oito tipos de ruído; um relatório opcional faz o
mesmo com uma gravação da sua voz, que fica fora do repositório. Testes da interface e
de hardware têm procedimentos próprios em [Desenvolvimento](docs/development.md#testes).
Mantenha os guias atualizados antes de alterar este resumo.

## Licença

O repositório ainda não define uma licença de distribuição.
