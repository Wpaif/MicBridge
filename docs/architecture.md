# Arquitetura

## Caminho do áudio

```mermaid
flowchart TD
    Phone[Navegador do celular] -->|HTTPS: pareamento e SDP| Server[Servidor local]
    Phone -->|WebRTC / Opus| Decode[Decoder Opus]
    Decode --> Original[Captura original opcional]
    Decode --> DSP[Tratamento mono a 48 kHz]
    DSP --> Treated[Captura tratada opcional]
    DSP --> Monitor[Retorno local opcional]
    DSP --> Mute[Mute da saída]
    Mute --> Sink[pw-cat: vdo_input]
    Sink --> Source[pw-loopback: MicBridge]
    Source --> App[Aplicativo de chamada / gravação]
```

O processamento ocorre antes do mute e da escrita no microfone virtual. Captura
A/B e retorno recebem o áudio antes do mute. O retorno usa uma saída selecionada
e volume próprio; não é reprodução no celular.

A cadeia DSP é: passa-altas → RNNoise → gate → de-esser → presença → equalizador
de 3 bandas → compressor → ganho automático → ganho manual → limitador. O RNNoise usa frames de 480 amostras
(10 ms a 48 kHz); o decoder aceita pacotes Opus de até 120 ms. A comparação A/B
preserva os pontos de captura, sem compensar o atraso interno do denoiser.

## Módulos

| Responsabilidade | Arquivos |
|---|---|
| Aplicação, runtime e autostart | [main.rs](../app/src/main.rs) |
| Estado da sessão e pareamento | [state.rs](../app/src/state.rs) |
| HTTPS, WebRTC e emissão de áudio | [server.rs](../app/src/server.rs) |
| Página do celular e autenticação web | [web.rs](../app/src/web.rs), [app/web](../app/web/) |
| Certificados locais | [tls.rs](../app/src/tls.rs) |
| Processamento e métricas dos efeitos | [dsp.rs](../app/src/dsp.rs), [dsp/effects.rs](../app/src/dsp/effects.rs) |
| Microfone virtual, mDNS e recuperação da saída | [output.rs](../app/src/output.rs) |
| Captura, WAV e reprodução A/B | [recording.rs](../app/src/recording.rs), [recording](../app/src/recording/) |
| Coordenação de reprodução e retorno | [local_audio.rs](../app/src/local_audio.rs) |
| Retorno, dispositivo, fila e volume | [monitor.rs](../app/src/monitor.rs), [monitor](../app/src/monitor/) |
| Fonte extra (som de um app no microfone virtual) | [app_source.rs](../app/src/app_source.rs) |
| Exclusividade e restauração dos links | [monitor/routing](../app/src/monitor/routing/) |
| Perfis personalizados e preferências | [profiles.rs](../app/src/profiles.rs), [settings.rs](../app/src/settings.rs) |
| Textos e decisões que a interface mostra | [monitor_view.rs](../app/src/monitor_view.rs), [recording_view.rs](../app/src/recording_view.rs), [effect_view.rs](../app/src/effect_view.rs) |
| Daemon e protocolo da interface | [ipc.rs](../app/src/ipc.rs) |
| Interface QML (Quickshell) | [app/ui](../app/ui/) |
| Bandeja e ícone | [tray.rs](../app/src/tray.rs), [tray_icon.rs](../app/src/tray_icon.rs) |

Os arquivos `*_view.rs` traduzem o estado dos módulos de áudio no que a interface
mostra, sem widgets. Estado de transição de áudio não é confirmado apenas pelo
clique: a interface aguarda a conclusão das operações correspondentes.

### Interface

`micbridge` sobe o motor como daemon, com a bandeja, e abre um socket Unix em
`$XDG_RUNTIME_DIR/micbridge.sock` (0600). A janela é um processo `qs -p app/ui`
que conversa com ele por JSON, uma mensagem por linha: o daemon envia `qr` e
`state` quando mudam (no máximo 10 vezes por segundo) e a UI envia comandos como
`{"c":"mute","on":true}` ou `{"c":"profile","id":"chamada"}`; a lista completa está
no topo de `ipc.rs`. Uma segunda execução encontra o socket e só pede `show`.

A janela não guarda regras próprias: apresentação do retorno, do teste de voz e
dos indicadores de efeito vem de `monitor_view.rs`, `recording_view.rs` e
`effect_view.rs`, e a edição de perfis usa `profiles::Library`. Confirmações
(substituir WAVs, descartar alterações, excluir perfil) aparecem dentro da janela.
A opção antiga `--quickshell` ainda é aceita e ignorada, para autostarts antigos.

## Concorrência e recuperação

O runtime Tokio executa rede, processos de áudio e o socket da interface; a janela
é um processo `qs` separado. Parâmetros DSP e leituras rápidas usam atômicos. Gravação e
retorno recebem cópias por filas limitadas: falha/atraso desses consumidores pode
interromper o recurso local sem obrigar o produtor a esperar por ele.

O retorno exclusivo identifica nodes/portas do PipeWire e registra links antes
de removê-los. A restauração verifica a identidade atual dos objetos. Um lock
impede duas instâncias de controlar o mesmo registro de roteamento. A/B e retorno
são mutuamente exclusivos; gravação pode coexistir com retorno.

## Arquivos do aplicativo

| Arquivo ou diretório | Uso |
|---|---|
| `$XDG_DATA_HOME/micbridge/settings.json` | Perfis de áudio e seleção; chaves não relacionadas são preservadas ao atualizar. |
| `$XDG_DATA_HOME/micbridge/ui/` | Interface QML instalada. |
| `$XDG_DATA_HOME/micbridge/devices.json` | Dispositivos pareados e hashes dos segredos. |
| `$XDG_DATA_HOME/micbridge/monitor-routing.json` | Links pendentes de restauração do retorno. |
| `$XDG_DATA_HOME/micbridge/monitor-routing.lock` | Lock do registro de roteamento. |
| `$XDG_DATA_HOME/micbridge/certs/` | CA, chaves, certificado do servidor, IP e impressão digital. |
| `~/.config/autostart/dev.wilian.MicBridge.desktop` | Autostart gravado pelo aplicativo. |
| `~/.local/state/omarchy/current/theme/colors.toml` | Paleta Omarchy atual, lida pela interface. |

Padrões XDG: dados em `~/.local/share`, estado em `~/.local/state` e configuração
em `~/.config`. O autostart do app usa diretamente `~/.config`; o desinstalador
consulta `XDG_CONFIG_HOME`. Considere essa diferença ao usar caminhos personalizados.

Dados de execução não pertencem ao repositório. A estrutura dos arquivos de
desenvolvimento está em [Desenvolvimento](development.md#arquivos-ocultos-e-locais).
