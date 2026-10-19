<div align="center">
  <img src="app/data/dev.wilian.MicBridge.svg" width="128" alt="MicBridge"/>

  <h1>MicBridge</h1>

  <p><strong>Use o celular como microfone do PC, com tratamento de voz em tempo real.</strong></p>

  <p>
  <a href="https://github.com/Wpaif/MicBridge/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Wpaif/MicBridge/ci.yml?style=flat-square&labelColor=0d1226&label=CI&logo=githubactions&logoColor=white" alt="CI"/></a>
  <img src="https://img.shields.io/badge/plataforma-Linux-f472b6?style=flat-square&labelColor=0d1226&logo=linux&logoColor=white" alt="plataforma Linux"/>
  <img src="https://img.shields.io/badge/Rust-2024-a78bfa?style=flat-square&labelColor=0d1226&logo=rust&logoColor=white" alt="Rust 2024"/>
  <img src="https://img.shields.io/badge/GTK-4.16%2B-818cf8?style=flat-square&labelColor=0d1226&logo=gtk&logoColor=white" alt="GTK 4.16+"/>
  <img src="https://img.shields.io/badge/libadwaita-1.6%2B-60a5fa?style=flat-square&labelColor=0d1226&logo=gnome&logoColor=white" alt="libadwaita 1.6+"/>
  <img src="https://img.shields.io/badge/%C3%A1udio-PipeWire-38bdf8?style=flat-square&labelColor=0d1226&logo=pipewire&logoColor=white" alt="áudio PipeWire"/>
  <img src="https://img.shields.io/badge/WebRTC-Opus-22d3ee?style=flat-square&labelColor=0d1226&logo=webrtc&logoColor=white" alt="WebRTC Opus"/>
  </p>
</div>

Use o celular como microfone do PC pela rede local, com tratamento de voz em
tempo real. O áudio sai do navegador via WebRTC/Opus e chega ao **MicBridge**,
uma entrada virtual do PipeWire que pode ser selecionada no Discord, OBS e outros
programas. Não é necessário instalar um app no celular.

Interface para Linux em Rust, GTK4 e libadwaita, com status fixo, conexão lateral
e painéis de tratamento e teste organizados por contexto.

## Recursos

- Perfis Natural, Chamada, Podcast, Streaming e Original, além de perfis locais
  personalizados com salvamento explícito.
- RNNoise, filtro de graves, gate, de-esser, presença, compressor, volume automático
  e limitador, com medidores e indicadores de atuação dos efeitos.
- Teste de voz de até 30 segundos, comparação original/tratado e exportação WAV.
- Retorno no fone com saída escolhida, volume próprio e ajuste exclusivo que
  restaura os links de áudio ao terminar.
- Mute da saída virtual, preservando retorno e gravação de teste.
- Pareamento por QR, HTTPS com CA local, reconexão e anúncio mDNS opcional.
  Um celular já pareado pode ler o QR novamente só para abrir a interface, sem
  reabrir o pareamento. Vários celulares podem ser pareados, mas só um fica
  conectado por vez.
- Temas claro/escuro/personalizado, transparência e acompanhamento automático da
  paleta Omarchy no modo sistema.
- Deslizantes protegidos contra alterações acidentais durante a rolagem.

Os recursos acima descrevem esta linha de desenvolvimento, não uma release
publicada. Consulte o [plano de evolução](docs/roadmap.md) para itens pendentes.

## Instalação

Requer Linux com PipeWire, Rust/Cargo, GTK ≥ 4.16, libadwaita ≥ 1.6, libopus e
ferramentas de compilação/áudio. A instalação também usa `rsvg-convert`.

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
5. Ajuste um perfil de voz e use A/B ou retorno no fone para comparar.

O navegador precisa aceitar o HTTPS. Consulte [Certificados](docs/security.md#certificados).
O retorno exclusivo desconecta temporariamente outros aplicativos da saída
selecionada; a parada tenta restaurá-los. Fechar a janela só a esconde; use
**Sair** na bandeja para encerrar o app.

Instruções completas de perfis, mute, testes, retorno e temas estão no
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
| Oito melhorias planejadas | [Evolução](docs/roadmap.md) |

## Desenvolvimento

```sh
cargo build --offline --manifest-path app/Cargo.toml
cargo test --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
```

O modo offline exige dependências previamente disponíveis. Testes GTK e de
hardware têm procedimentos próprios em [Desenvolvimento](docs/development.md#testes).
Mantenha os guias atualizados antes de alterar este resumo.

## Licença

O repositório ainda não define uma licença de distribuição.
