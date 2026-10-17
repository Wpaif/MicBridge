# Desenvolvimento

Execute os comandos deste guia a partir da raiz do repositório. Para dependências
de compilação e execução, consulte [Instalação](installation.md).

## Estrutura

```text
app/
  src/          Aplicação Rust, DSP, rede, modelos e interface
  web/          Página do celular, embutida no binário
  data/         CSS, ícone, atalho e paleta de exemplo
  tests/        Auxiliares Python dos testes de entrada GTK
  Cargo.toml    Dependências e recursos habilitados
  Cargo.lock    Resolução de dependências versionada
docs/           Guias atuais de uso e manutenção
scripts/        Instalação e desinstalação por usuário
.claude/skills/ Instruções locais de design para ferramentas compatíveis
```

O mapa dos módulos está em [Arquitetura](architecture.md#módulos).

## Compilar e executar

```sh
cargo build --offline --manifest-path app/Cargo.toml
cargo run --manifest-path app/Cargo.toml
```

`--offline` exige que as dependências estejam no cache. Na primeira compilação,
execute sem essa opção se precisar obtê-las. Use `--release` para o build de
distribuição; o instalador também compila em release.

O app executado pelo Cargo é completo: inicia rede e áudio. O modo debug aceita
`MICBRIDGE_APP_ID` para usar outro ID de aplicação e `XDG_DATA_HOME` para dados
temporários. Isso evita compartilhar preferências e pareamento, mas não isola
automaticamente as portas de rede nem os nodes PipeWire de outra instância.

## Testes

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```

A suíte comum cobre DSP, perfis, persistência, captura, WAV, filas e coordenação de
processos/roteamento com fixtures. Uma aprovação não equivale a uma validação
auditiva com celular, Bluetooth ou aplicação de chamada.

Testes gráficos e o teste Bluetooth são ignorados por padrão. Liste-os com:

```sh
cargo test --offline --manifest-path app/Cargo.toml -- --ignored --list
```

Execute cada teste GTK em **um processo separado**, usando filtro e uma única
thread. Executar todos os ignorados juntos não é o procedimento de validação:
GTK exige afinidade de thread e alguns testes também alteram variáveis XDG.

```sh
G_DEBUG=fatal-warnings cargo test --offline --manifest-path app/Cargo.toml connection_sidebar_stays -- --ignored --test-threads=1
```

Filtros úteis:

| Filtro | Verificação |
|---|---|
| `studio_panels_use` | Colunas, larguras, agrupamento de A/B/retorno e maximização. |
| `connection_sidebar_stays` | Lateral fixa, diálogo compacto e retorno dos controles. |
| `fixed_monitoring_header` | Status e medidores fora da rolagem. |
| `custom_audio_profiles_follow` | CRUD, restauração, dirty e erros de escrita. |
| `mute_control` | Estado e posição do mute. |
| `appearance_dialog_previews` | Prévia, cancelamento e persistência visual. |
| `panel_controls` | Controles do retorno e encerramento da janela. |

Para roda, teclado e arraste reais, em uma sessão Wayland:

```sh
python3 app/tests/run_slider_scroll.py
```

Esse runner usa Xwayland rootful isolado, Python padrão e as bibliotecas
libX11/libXtst. Encerra o servidor ao terminar. Em X11 normal, também é possível
executar diretamente o filtro `slider_wheel_scrolls` com `GDK_BACKEND=x11`.
Essas ferramentas não são novas dependências de execução do MicBridge.

O teste `bluetooth_live_monitor` exige dispositivo e PipeWire reais. Ele
reproduz áudio e pode modificar os links da saída selecionada. Não trate sua
ausência numa suíte comum como aprovação de hardware.

## Prévia visual e logs

Os testes de layout aceitam `MICBRIDGE_TEST_SNAPSHOTS=1` para salvar capturas
em `/tmp`; consulte o teste executado para os nomes e cenários das capturas.

No binário de debug, `MICBRIDGE_SNAPSHOT` salva uma captura após a abertura e
encerra; `MICBRIDGE_SNAPSHOT_SIZE=LARGURAxALTURA` escolhe o tamanho, e
`MICBRIDGE_DEMO` simula a apresentação do status. Essas variáveis não substituem
testes reais de conexão.

Mensagens do servidor, áudio e persistência aparecem ao executar o programa no
terminal. O projeto não configura um arquivo de log permanente por padrão.

## Arquivos ocultos e locais

| Caminho | Política |
|---|---|
| `.git/` | Metadados Git; gerenciados pelo Git. |
| `.gitignore` | Regras centrais para build, editor e temporários. |
| `.claude/skills/libadwaita-design/SKILL.md` | Instrução versionada; caminho preservado para descoberta pela ferramenta. |
| `.zed/` | Preferências locais, ignoradas; não são reescritas pela organização do projeto. |
| `.superpowers/` | Scratch/cache local ignorado. |
| `app/target/` | Build e cache Cargo, ignorados. |
| `__pycache__/` e `*.pyc` | Cache Python, ignorado. |

Diretórios como `.agents`, `.codex` e `.aws` podem ser fornecidos pelo ambiente de
trabalho. Não são documentação do produto; não os mova nem reorganize credenciais
para dentro do repositório. Esta reorganização não altera esses diretórios.

## Fluxo de alterações

Use branch por funcionalidade e commits no padrão já adotado, como `feat(audio):`,
`fix(ui):` e `docs:`. Para mudanças de comportamento, escreva uma regressão,
observe a falha e implemente a correção antes da validação completa. Registre
separadamente o que foi automatizado e o que dependeu de verificação manual.

Atualize o guia afetado e, quando necessário, o design; só depois revise o resumo
no README. O histórico de decisões e implementações pode ser consultado no Git.
