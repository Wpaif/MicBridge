# Instalação

## Requisitos

O aplicativo é destinado a Linux com PipeWire e sessão gráfica. Para compilar:

- Rust/Cargo com suporte à edição 2024 e às dependências do projeto.
- Bibliotecas de desenvolvimento GTK 4.16 ou superior, libadwaita 1.6 ou superior
  e libopus, acessíveis ao `pkg-config`.
- `rsvg-convert`, do librsvg, para gerar os ícones durante a instalação.

O áudio usa `pw-cat` e `pw-loopback`. O retorno exclusivo também usa `pw-dump`,
`pw-link` e `pw-cli`. Instale as ferramentas do PipeWire fornecidas pela sua
distribuição. O qpwgraph não é necessário.

O avahi é opcional: `avahi-publish` e seu serviço permitem anunciar
`micbridge.local`. Sem ele, use o IP mostrado no aplicativo. A bandeja usa
StatusNotifier; no GNOME pode ser necessária uma extensão compatível.

Os nomes dos pacotes variam por distribuição. As dependências Rust e seus
requisitos estão em [Cargo.toml](../app/Cargo.toml) e [Cargo.lock](../app/Cargo.lock).

## Instalar ou atualizar

Para obter o código e instalar:

```sh
git clone https://github.com/Wpaif/MicBridge.git
cd MicBridge
./scripts/install.sh
```

O script compila em release, encerra a instância instalada em execução e instala:

| Arquivo | Destino |
|---|---|
| Executável | `~/.local/bin/micbridge` |
| Atalho | `$XDG_DATA_HOME/applications/dev.wilian.MicBridge.desktop` |
| Ícones SVG e PNG | `$XDG_DATA_HOME/icons/hicolor/` |

| Interface QML (só Quickshell) | `$XDG_DATA_HOME/micbridge/ui/` |

### Interface: GTK ou Quickshell

O instalador escolhe a interface pelo ambiente. Em tiling WMs (Hyprland, Sway,
niri, river...) usa a interface [Quickshell](https://quickshell.org), inspirada no
[Flea](https://github.com/thisisgm/flea), que segue as cores do tema Omarchy. Em
GNOME, KDE e outros desktops completos usa GTK4/libadwaita. Sem o `qs` instalado,
cai para GTK. Para forçar uma delas:

```sh
./scripts/install.sh --ui=quickshell   # ou --ui=gtk
```

No modo Quickshell o atalho executa `micbridge --quickshell`: o motor roda como
daemon com a bandeja e a janela é um processo `qs` à parte. Fechar a janela não
desliga o microfone; use **Sair** na bandeja. Abrir o MicBridge com a janela já
aberta a traz para frente (Hyprland e Sway).

No Omarchy, a janela segue o layout da área de trabalho, como os outros apps:
fica lado a lado em `dwindle` e `scrolling` e flutua em `floating`
(`omarchy hyprland workspace layout`). Versões antigas punham um bloco em
`~/.config/hypr/bindings.lua` que forçava a janela a flutuar; o instalador e o
desinstalador removem esse bloco, sem tocar no resto do arquivo. Se o Hyprland
acusar erro depois de recarregar, o arquivo anterior volta.

A interface Quickshell tem os mesmos recursos da GTK, exceto o diálogo de
aparência: as cores seguem o tema atual do Omarchy e mudam junto com ele. Fora do
Omarchy, usa uma paleta escura padrão. Atalhos de teclado:

| Tecla | Ação |
|---|---|
| `M` | Silenciar ou reativar o microfone virtual |
| `P` | Aceitar novo celular |
| `1`–`9` | Escolher o perfil de voz |
| `N` / `S` / `R` / `Del` | Salvar como novo, salvar alterações, renomear, excluir perfil |
| `Esc` | Fechar a confirmação aberta ou, sem nenhuma, a janela |

Quando `XDG_DATA_HOME` não está definido, o padrão é `~/.local/share`.
Abra **MicBridge** pelo menu de aplicativos. A atualização comum preserva os
dados locais, incluindo perfis, pareamento e certificados.

## Reinstalar ou remover

| Comando | Resultado |
|---|---|
| `./scripts/install.sh --reinstall` | Remove dados do app e o build, recompila e reinstala; preserva a CA local. |
| `./scripts/install.sh --reinstall --new-ca` | Também substitui a CA; os celulares precisam confiar na nova CA. |
| `./scripts/uninstall.sh` | Remove binário, atalho, ícones, autostart, interface QML e a regra de janela do Hyprland; preserva os dados do app. |
| `./scripts/uninstall.sh --purge` | Remove também perfis, preferências, pareamento, certificados e demais dados do app. |
| `./scripts/uninstall.sh --purge --keep-ca` | Remove os dados, preservando apenas `ca.pem` e `ca.key`. |

As operações de limpeza pedem confirmação; `--yes` pula essa confirmação. Leia o
efeito antes de usar: uma reinstalação limpa não preserva os perfis personalizados.
Os scripts não removem o código-fonte nem pacotes do sistema.

## Executar sem instalar

```sh
cargo run --release --manifest-path app/Cargo.toml
```

Isso inicia o aplicativo completo, com servidor HTTPS e microfone virtual. Não
é uma prévia isolada. Para desenvolvimento e dados temporários, consulte
[Desenvolvimento](development.md).

## Próximo passo

Siga o [guia de uso](usage.md#primeira-conexão). A instalação do programa no PC
não instala automaticamente a CA no celular; veja [Certificados](security.md#certificados).
