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
- 🔐 **Pareamento seguro**: token no QR code, janela de pareamento temporária e um celular por vez
- 🔏 **TLS com CA local**: certificado próprio e impressão digital visível no app

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

1. Abra o **MicBridge**. O microfone virtual e o servidor web sobem sozinhos, e a janela de
   pareamento fica aberta por 5 minutos.
2. No celular (mesma rede), **escaneie o QR code** (botão de QR na janela) ou cole o link copiado.
   Abrir só `https://<IP>:3443` não funciona: o link precisa do token.
3. Na primeira vez, instale a CA em `https://<IP>:3443/ca.crt` ou aceite o aviso do certificado
   conferindo a impressão digital mostrada no app. Toque em **Iniciar microfone**.
4. No Discord/OBS, escolha **Microfone_VDO** como entrada.

Para parear outro celular, ligue **Aceitar novo celular** na janela. Ao reiniciar o app o token
muda, então é preciso escanear o QR de novo.

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

- **Pareamento por token:** a cada abertura o app gera um token aleatório (128 bits) embutido
  no QR code. Quem abre o link recebe um cookie de sessão assinado (`Secure`, `HttpOnly`,
  `SameSite=Strict`); sem ele, a página e o `/offer` respondem 403. O Rust também valida o token.
  Como o token muda a cada execução, é preciso **escanear o QR de novo** quando o app reinicia.
- **Um celular por vez:** enquanto um dispositivo estiver conectado, outro recebe 409. O mesmo
  celular pode reconectar (recarregar a página) e assume a própria conexão.
- **Janela de pareamento:** novos celulares só entram enquanto a opção **Aceitar novo celular**
  estiver ativa (abre por 5 min ao iniciar o app e fecha sozinha após o primeiro pareamento).
  Com ela fechada, um QR vazado não serve; só celulares já pareados reconectam. O Rust é quem
  decide (`/pair`), não o Rails.
- **TLS com CA local:** o `web-https.sh` cria a *MicBridge Local CA* e assina o certificado do
  servidor com ela. Instale `web/tmp/certs/ca.pem` no celular uma vez (ou baixe em
  `https://<IP>:3443/ca.crt`) e o navegador passa a confiar sem avisos, o que protege contra
  alguém se passando pelo servidor. A chave da CA nunca sai do PC. O certificado é refeito
  sozinho quando o IP muda. Sem instalar a CA, confira a **impressão digital SHA-256** mostrada no
  app com a do aviso do navegador.
- **Superfície de rede:** o Rails escuta só na interface da LAN e no loopback (não em `0.0.0.0`);
  o servidor WebRTC escuta apenas em `127.0.0.1`.
- **Camada de rede (fora do app):** para isolamento forte, use uma VLAN/rede de convidados
  separada, regra de firewall liberando a porta 3443 só para o IP do celular, ou uma VPN
  (ex.: Tailscale).

## 📄 Licença

Projeto pessoal, sem licença definida.
