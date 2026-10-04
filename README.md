<div align="center">

<img src="app/data/dev.wilian.MicBridge.svg" width="128" alt="MicBridge"/>

# MicBridge

**Use o celular como microfone do PC, com áudio tratado em tempo real.**

![Rust](https://img.shields.io/badge/Rust-GTK4%20%2B%20libadwaita-dea584?logo=rust&logoColor=white)
![Rails](https://img.shields.io/badge/Rails-8-cc0000?logo=rubyonrails&logoColor=white)
![WebRTC](https://img.shields.io/badge/WebRTC-Opus-7c5cff)
![PipeWire](https://img.shields.io/badge/PipeWire-Linux-22d3ee)

</div>

---

## ✨ O que é

O MicBridge transforma o celular em um microfone sem fio para o PC, **só na rede local**.
Você abre uma página no celular (via QR code), o áudio viaja por **WebRTC** com baixa latência
e o app em Rust trata o som antes de entregá-lo a um microfone virtual do PipeWire, que você
seleciona no Discord, OBS, Meet etc.

## 🎛️ Recursos

- 📱 **Celular como microfone** pela página web, sem instalar app no celular
- ⚡ **Baixa latência** com WebRTC (Opus sobre UDP, sem STUN, apenas LAN)
- 🔇 **Cancelamento de ruído** com RNNoise
- 🚪 **Noise gate** com hold e limiar ajustável
- 🗜️ **Compressor** (limiar, razão) e **ganho final** com limitador suave
- 📊 **Medidores** de entrada e saída em tempo real
- 🔳 **QR code** para conectar rapidamente
- 🖥️ **Interface GTK4 + libadwaita** nativa do GNOME
- 🔔 **Ícone na bandeja** que muda quando o celular conecta; fechar a janela só a esconde
- 🚀 **Servidor web automático**: sobe com o app e para quando ele fecha

## 🧭 Como funciona

```
 Celular (Rails + Stimulus, HTTPS)
        │  SDP offer
        ▼
     Rails ──── POST /whip ────▶ App Rust (GTK4 / libadwaita)
        ▲                              │
        └── áudio Opus via WebRTC ─────┤  decode
              (UDP, LAN)               ▼
                                RNNoise ▶ gate ▶ compressor ▶ limitador
                                       │
                                       ▼ pw-cat
                              sink  vdo_input
                                       │
                                       ▼
                           🎤 fonte  "Microfone_VDO"
```

| Pasta | Conteúdo |
|-------|----------|
| `app/` | App Rust: UI, servidor WHIP (axum), WebRTC (`webrtc` 0.21), DSP, bandeja |
| `web/` | Rails: página do celular e proxy do SDP (HTTPS é obrigatório para usar o microfone) |
| `scripts/` | `install.sh` (instala app + atalho) e `web-https.sh` (Rails em HTTPS na LAN) |

## 📋 Requisitos

- Linux com **PipeWire** (`pw-cat`, `pw-loopback`)
- **Rust** (edição 2024), **GTK 4**, **libadwaita**, **libopus**
- **Ruby 4** e **Bundler** (Rails 8)
- `openssl` e `librsvg` (`rsvg-convert`) para instalar
- GNOME: extensão **AppIndicator** para o ícone da bandeja

## 🚀 Instalação

```sh
git clone git@github.com:Wpaif/MicBridge.git
cd MicBridge
(cd web && bundle install)
./scripts/install.sh
```

Isso compila o app e instala binário, ícones e o atalho no menu de aplicativos.
Procure por **MicBridge**.

## ▶️ Uso

1. Abra o **MicBridge**. O microfone virtual e o servidor web sobem sozinhos.
2. Clique no botão de QR code (ou abra a URL `https://<IP-do-PC>:3443`) no celular, na mesma rede.
3. Aceite o aviso do certificado autoassinado e toque em **Iniciar microfone**.
4. No Discord/OBS, escolha **Microfone_VDO** como entrada.

Para encerrar de verdade, use **Sair** no menu da bandeja. O botão "X" apenas esconde a janela.

### Desenvolvimento

```sh
cd app && cargo run --release   # app (sobe o Rails sozinho)
```

Se mover a pasta do projeto, defina `MICBRIDGE_HOME` com o novo caminho.

## 🛠️ Ajustes de áudio

| Controle | Efeito |
|----------|--------|
| Cancelamento de ruído | Remove ruído de fundo (RNNoise) |
| Noise gate / limiar | Silencia abaixo do nível definido |
| Compressor / limiar / razão | Nivela a dinâmica da voz |
| Ganho final | Compensa o volume; passa por um limitador suave |

## 🔒 Segurança

O Rails escuta na rede local (porta 3443) e o servidor WebRTC apenas em `127.0.0.1`.
Qualquer pessoa na mesma rede que abrir a URL pode enviar áudio, então use em redes confiáveis.

## 📄 Licença

Projeto pessoal, sem licença definida.
