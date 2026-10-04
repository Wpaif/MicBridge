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
- 🔁 **Reconexão automática**: Wi-Fi caiu ou tela bloqueou? O celular volta sozinho, sem novo QR
- 💾 **Pareamento persistente**: QR só na primeira vez; depois o celular é reconhecido
- 🚦 **Estado sempre visível**: conectado, desconectado, reconectando ou erro (janela e bandeja)
- 🩹 **PipeWire autorrecuperável**: se o microfone virtual sumir, é recriado sem reiniciar nada
- 🏷️ **Nome fixo `micbridge.local`** via mDNS (avahi): o endereço não muda mesmo que o IP mude
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
- **avahi** (opcional, recomendado) para o nome fixo `micbridge.local`: `sudo systemctl enable --now avahi-daemon`
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

**Primeira vez**
1. Abra o **MicBridge**: o microfone virtual e o servidor sobem sozinhos e o QR code já aparece.
2. No celular (mesma rede), escaneie o QR e toque em **Iniciar microfone**. Pronto.
3. No Discord/OBS, escolha **Microfone_VDO** como entrada.

**Depois**
Abra o MicBridge (ou ative **Iniciar com o sistema** para ele subir escondido na bandeja) e abra
a página no celular: ele é reconhecido e o áudio volta sozinho. Dica: adicione a página à tela
inicial do celular.

O estado aparece no topo da janela e no ícone da bandeja: **Conectado**, **Reconectando**
(a conexão caiu; o celular tenta voltar sozinho), **Desconectado** ou **Erro**.

**Nome fixo (mDNS):** com o `avahi-daemon` ativo, o app anuncia `micbridge.local` e o QR passa a
usar `https://micbridge.local:3443`. O cookie de pareamento é por endereço, então com o nome fixo
ele continua valendo mesmo que o IP do PC mude por DHCP. Sem o avahi, o app usa o IP (a janela
indica qual dos dois está em uso) e uma mudança de IP exige parear de novo; nesse caso, reserve o
IP no roteador.

> Limitações: navegadores móveis podem suspender o microfone com a tela bloqueada (a página mantém
> a tela acesa com Wake Lock enquanto transmite); alguns Androids antigos não resolvem `.local`
> (use o IP); o servidor web escuta no IP de quando subiu, então reabra o app se o IP mudar.

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
  no QR code, aceito só com a janela de pareamento aberta. Ao parear, o app entrega ao celular um
  id e um segredo de longa duração (256 bits) guardados num cookie (`Secure`, `HttpOnly`,
  `SameSite=Strict`, 1 ano). O token do QR muda a cada execução, mas o celular já pareado
  continua reconhecido: não precisa escanear de novo. O Rails fala com o app por um token interno
  que o celular nunca vê.
- **Um celular por vez:** enquanto um dispositivo estiver conectado, outro recebe 409. O mesmo
  celular pode reconectar (recarregar a página) e assume a própria conexão.
- **Janela de pareamento:** novos celulares só entram enquanto a opção **Aceitar novo celular**
  estiver ativa. Ela abre por 5 min só quando não há nenhum celular pareado (primeira utilização) e
  fecha sozinha após cada pareamento. As credenciais de cada celular ficam em disco só como hash
  (`~/.local/share/micbridge/devices.json`, modo 600); **Esquecer todos** revoga tudo.
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
