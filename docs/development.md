# Desenvolvimento

Execute os comandos deste guia a partir da raiz do repositório. Para dependências
de compilação e execução, consulte [Instalação](installation.md).

## Estrutura

```text
app/
  src/          Daemon Rust: DSP, rede, áudio e protocolo da interface
  ui/           Interface QML (Quickshell); ui/tests tem os testes de QML
  web/          Página do celular, embutida no binário
  data/         Ícone e atalho .desktop
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

O app executado pelo Cargo é completo: inicia rede, áudio e a janela. Para usar a
interface do repositório em vez da instalada, aponte `MICBRIDGE_UI_DIR` para ela:

```sh
MICBRIDGE_UI_DIR=$PWD/app/ui cargo run --manifest-path app/Cargo.toml
```

Para testar com o celular, use `--release`: o build de desenvolvimento não é
otimizado e pode cortar o áudio recebido (o original do teste de voz já chega
cortado).

O Quickshell recarrega a janela sozinho ao salvar um `.qml`; mudanças no Rust
pedem reiniciar o daemon. `XDG_DATA_HOME` aponta os dados para outro lugar, mas
não isola as portas de rede, o socket nem os nodes PipeWire de outra instância:
encerre a instalada antes.

## Testes

O workflow [Testes automatizados](../.github/workflows/ci.yml) executa em pushes,
pull requests e acionamento manual na aba **Actions** do GitHub. Ele verifica a
formatação, recusa avisos do clippy e executa `cargo test --locked`, compilando com Rust estável e as
dependências fixadas em `Cargo.lock`. As dependências Cargo ficam em cache.

O ambiente usa Debian Trixie em contêiner. Testes que exigem hardware continuam
ignorados nessa execução; os procedimentos abaixo permitem executá-los localmente.

```sh
cargo test --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
cargo clippy --offline --manifest-path app/Cargo.toml --all-targets -- -D warnings
git diff --check
```

A suíte comum cobre DSP, perfis, persistência, captura, WAV, filas e coordenação de
processos/roteamento com fixtures.

Os testes de qualidade do DSP (`app/src/dsp_quality.rs`) passam uma voz sintética
pela cadeia misturada a oito ruídos: as cores branco, rosa, marrom, azul e violeta
(que diferem pela inclinação do espectro), zumbido de 60 Hz, cliques de
teclado e vozes ao fundo. Eles exigem que o tratamento separe voz de
ruído, leve a voz ao nível-alvo, respeite o teto do limitador, que o compressor atue
e que os perfis soem diferentes entre si.

Para medir com a sua voz, grave um **Teste de voz** num lugar silencioso, use
**Exportar WAVs** e aponte para o original. O relatório mistura cada ruído à
gravação e mostra a separação por perfil, quanto da fala cada perfil e cada efeito
cortam e o efeito dos atalhos do EQ. Com `MICBRIDGE_CUTS=cortes.wav`, grava os
trechos que a Chamada cortou, para ouvir se eram fala ou só respiração. A gravação
fica fora do repositório (que é público):

```sh
MICBRIDGE_VOICE=voz-original.wav cargo test --release --manifest-path app/Cargo.toml \
  dsp_quality::real_voice -- --ignored --nocapture
```

Para medir a cadeia com áudio real, a bancada `dsp_bench` processa uma gravação
(f32 mono 48 kHz, cru) com vários perfis e grava as saídas para comparar:

```sh
MICBRIDGE_BENCH_IN=entrada.f32 MICBRIDGE_BENCH_OUT=pasta \
  cargo test --release --manifest-path app/Cargo.toml dsp_bench -- --ignored --nocapture
```

Uma aprovação não equivale a uma validação auditiva com celular, Bluetooth ou aplicação de chamada.

O teste Bluetooth é ignorado por padrão. Liste os ignorados com:

```sh
cargo test --offline --manifest-path app/Cargo.toml -- --ignored --list
```

Os testes de QML ficam em `app/ui/tests` e rodam com o `qmltestrunner` do Qt 6 (o do `PATH` pode ser outro e sair sem mensagem):

```sh
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input app/ui/tests
```

O teste `bluetooth_live_monitor` exige dispositivo e PipeWire reais. Ele
reproduz áudio e pode modificar os links da saída selecionada. Não trate sua
ausência numa suíte comum como aprovação de hardware.

## Traduções

A página do celular escolhe o idioma pelo `Accept-Language` do navegador; sem
acerto, usa inglês. Os textos ficam em `app/web/i18n/<tag>.json`, um objeto plano
`chave -> texto`, com variáveis em `{nome}`. O servidor usa o mesmo arquivo nas
páginas que monta (`<!--T:chave-->`) e o embute na página para o `app.js` (`t("chave")`).

A janela segue o idioma do sistema (`LC_ALL`, `LC_MESSAGES` ou `LANG`), também
com inglês como padrão. Os textos ficam em `app/ui/i18n/<tag>.json`, no mesmo
formato, e o `I18n.qml` oferece `I18n.t("chave", { variavel })`, `I18n.plural`
(chaves `.one` e `.other`) e `I18n.num`, que usa o separador decimal do idioma.
Textos que o daemon envia prontos (estado da conexão, mensagens, nomes de perfis)
ainda saem em português.

Para adicionar um idioma, crie o JSON com a tag nova nas duas pastas, traduza os
valores e registre a tag em `SOURCES` (`app/src/i18n.rs`) e em `available`
(`app/ui/I18n.qml`). Os testes recusam chave faltando, variável diferente entre
idiomas e chave usada sem texto, na página e na janela.

## Prévia visual e logs

Para conferir a interface, rode o daemon com `MICBRIDGE_UI_DIR` (acima) e capture
a janela. Sem celular conectado os medidores ficam parados: uma captura não
substitui testes reais de conexão e áudio.

Mensagens do servidor, áudio e persistência aparecem ao executar o programa no
terminal. O projeto não configura um arquivo de log permanente por padrão.

## Arquivos ocultos e locais

| Caminho | Política |
|---|---|
| `.git/` | Metadados Git; gerenciados pelo Git. |
| `.gitignore` | Regras centrais para build, editor e temporários. |
| `.claude/skills/interface-design/SKILL.md` | Instrução versionada para quem altera a interface ou a página do celular. |
| `.zed/` | Preferências locais, ignoradas; não são reescritas pela organização do projeto. |
| `.superpowers/` | Scratch/cache local ignorado. |
| `app/target/` | Build e cache Cargo, ignorados. |

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
