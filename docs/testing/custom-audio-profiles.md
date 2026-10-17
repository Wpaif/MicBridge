# Validação dos perfis personalizados de áudio

Branch `feat/custom-audio-profiles`, base `2721b31` (mute e aparência ainda em
branches de funcionalidades). Sem importação/exportação e sem dependências novas.

## Entrega

Seletor único com cinco perfis de fábrica e até 32 personalizados. Salvar como
novo captura os sete switches e quatro valores numéricos do tratamento. Salvar
alterações atualiza somente o personalizado selecionado; renomear mantém ajustes
pendentes; excluir pede confirmação e ativa Chamada. Perfis de fábrica protegidos
na interface e no modelo. Nomes únicos, sem controles, de 1–60 caracteres.

A seleção e os ajustes salvos são restaurados ao iniciar. Preferência antiga
`preset` continua compatível. Mute, retorno, volume do retorno, dispositivos,
medidores e aparência não pertencem aos perfis. Trocas não reiniciam conexão/DSP.
Alterar controles só muda o áudio ao vivo; salvar exige ação explícita. Troca com
ajustes pendentes oferece Cancelar/Descartar. A gravação em disco ocorre em worker;
controles envolvidos ficam bloqueados até a resposta. Modelo/seleção só são
confirmados depois da escrita. Falha no editor mantém nome, mensagem e retry.

Preferências são atualizadas com o mesmo lock e rename atômico usados pelos temas.
Chaves não relacionadas são preservadas. Bibliotecas inválidas/incompatíveis
bloqueiam ações de perfil, mantendo ajustes manuais utilizáveis. A escrita também
valida a biblioteca existente sob lock, preservando o arquivo mesmo se ele mudou
depois que a interface abriu. Não há restauração destrutiva automática.

## TDD e revisão

- Modelo: testes escritos antes da API; captura/aplicação dos 11 controles,
  preservação de mute/medidor, CRUD, seleção, proteção dos perfis de fábrica,
  rejeição de nomes duplicados/inválidos, versões e parâmetros fora dos limites.
- Persistência RED: loader inicial retornava Chamada em vez do perfil legado
  Podcast. GREEN com migração, roundtrip de personalizados/seleção, coexistência
  com aparência, erros e preservação de JSON danificado.
- GTK RED: seletor ausente. GREEN cobrindo criar/renomear/salvar, dirty sem escrita,
  cancelar/descartar troca, restaurar ajustes em controles, preservação do mute,
  falha real de disco mantendo editor e dados, reabrir, excluir e retorno a Chamada.
  Nome de 60 caracteres também cabe na janela de 420 px.
- Aviso crítico de foco reproduzido: ComboRow estava em Box. Corrigido usando
  ListBox, com regressão e repetição usando `G_DEBUG=fatal-criticals`.
- Revisão independente encontrou risco de sobrescrever biblioteca rejeitada.
  RED de tentativa de save comprovou o risco. GREEN com proteção no armazenamento
  e interface; regressão verifica arquivo byte a byte e edição bloqueada após
  carregar versão incompatível. Revisor confirmou a correção, sem outros pontos
  importantes.

## Resultados

123 testes comuns aprovados, zero falhas; 14 ignorados por padrão. Executados
explicitamente em processos separados os testes GTK `custom_audio_profiles_follow`,
`fixed_monitoring_header` e `mute_control`, todos aprovados com críticos fatais.
Layout verificado em 420×400 e 1200×800. Build offline sem warnings; fmt e diff
aprovados. Regressões de PCM dos cinco perfis existentes continuam aprovadas.
Não foi realizada validação auditiva com celular/chamada neste lote.

```sh
cargo test --offline --manifest-path app/Cargo.toml
G_DEBUG=fatal-criticals cargo test --offline --manifest-path app/Cargo.toml custom_audio_profiles_follow -- --ignored --test-threads=1
G_DEBUG=fatal-criticals cargo test --offline --manifest-path app/Cargo.toml fixed_monitoring_header -- --ignored --test-threads=1
G_DEBUG=fatal-criticals cargo test --offline --manifest-path app/Cargo.toml mute_control -- --ignored --test-threads=1
cargo build --offline --manifest-path app/Cargo.toml
cargo fmt --manifest-path app/Cargo.toml --check
git diff --check
```
