# Retorno ao vivo no fone

## Objetivo e organização

Segundo recurso, após gravação A/B validada manualmente pelo usuário.
Branch `feat/headphone-monitor`, derivada da `main` após o merge `8798d45`.
Implementar com TDD, revisão independente e merge local somente após testes
aprovados. Manter commits no padrão do projeto: tipo/escopo e descrição em português.

## Experiência de uso

Painel “Retorno no fone”, próximo ao teste de voz, com seletor de saída,
botão para atualizar dispositivos, controle de volume entre 0 e 100% e
interruptor para ouvir a voz tratada ao vivo. O retorno inicia desligado,
com volume em 25%. Configurações permanecem na sessão; reiniciar o app
não liga o retorno automaticamente.

O usuário escolhe explicitamente uma saída PipeWire antes de ativar. A lista
usa nomes amigáveis e armazena o identificador do node, sem confundir nomes
iguais. Excluir o sink do microfone virtual (`vdo_input`) e eventuais nodes
internos do próprio retorno. Não escolher outra saída automaticamente caso
o destino selecionado desapareça. Mostrar “Use fones para evitar que o som
volte ao microfone” e informar que há atraso de monitoramento.

A ativação exige conexão ativa e dispositivo selecionado ainda disponível.
O volume afeta exclusivamente o retorno: não modifica DSP, níveis enviados
às chamadas, gravações original/tratada nem os perfis de voz. Mudanças de
volume usam rampa de 5 ms para evitar degraus bruscos. Volume zero silencia
o retorno sem mudar o estado do microfone principal.

Trocar o destino desliga o retorno; o usuário pode ativá-lo no novo destino.
Desconectar o celular, remover o dispositivo, falhar a escrita/reprodução ou
encerrar o app também desliga o retorno. Mostrar erro local e permitir nova
tentativa explícita. Falhas do retorno não mudam o estado geral da conexão
nem impedem gravações e chamadas.

Ouvir uma gravação A/B encerra o retorno ao vivo antes de começar a reprodução.
Ativar o retorno encerra a reprodução A/B antes de começar. Essa coordenação
aguarda o encerramento do filho anterior fora da thread GTK e do pump de áudio;
nunca retoma automaticamente um modo interrompido. Gravar um teste pode
continuar enquanto o retorno estiver ligado, para ouvir os próprios ajustes.

## Caminho do áudio

Amostras mono normalizadas a 48 kHz, já tratadas pelo DSP, são copiadas para
uma fila limitada dedicada ao retorno, antes da escrita no microfone virtual.
O produtor consulta um identificador de sessão e usa envio sem espera.
Não faz consultas a dispositivos, chamadas a processos, I/O de áudio nem
espera por mutex de buffers/consumidor.

Cada entrada contém no máximo 480 amostras (10 ms); a fila comporta seis
entradas, no máximo 60 ms de áudio aguardando consumo. Blocos maiores do
decoder são divididos para o retorno; isso não altera o processamento ou a
escrita já existentes para a saída principal. Apenas amostras da geração
de conexão e sessão de retorno atuais são aceitas.

Se a fila encher, interromper o retorno e informar que ele não acompanhou o
áudio, sem acumular som atrasado nem bloquear o pump. O consumidor verifica
a validade da sessão antes de reproduzir blocos; desativar descarta pendências.

Um worker dedicado escreve PCM f32 mono no `pw-cat`, com alvo explícito pelo
serial do node, sem fallback ou reconexão automática, taxa de 48000 Hz e pedido de latência de 20 ms. O pedido ao
PipeWire e o limite da fila não representam uma medição da latência total:
não exibir um número como se incluísse celular, rede, Opus, RNNoise e hardware.

O worker deve conseguir cancelar uma escrita bloqueada, matar e recolher o
processo filho. Reusar a proteção de morte com o pai já empregada no projeto.
Nenhum reinício automático deve reproduzir blocos de uma sessão anterior.

## Dispositivos e coordenação

Listar saídas com `pw-dump` e analisar o JSON com serde_json, já disponível.
Aceitar nodes de classe `Audio/Sink` com `node.name` válido. O destino deve
continuar identificado por nome e, durante uma ativação, por identidade do
node (serial), para detectar substituições após reconexões. Sem serial válido,
a abertura falha localmente em vez de usar um nome que possa ser reutilizado.
Listagem ocorre em worker, com timeout de 2 s e cancelamento do processo.
Dados inválidos/indisponibilidade geram mensagem no painel, sem travar GTK.

Atualizar na abertura do painel, mediante botão e a cada 2 s enquanto houver
retorno ativo. Não substituir uma seleção ainda válida ao atualizar a lista.
Resultados de consultas antigas não sobrescrevem consultas mais recentes.
Antes de abrir o fluxo, revalidar a seleção. Uma remoção detectada encerra
a sessão e mantém o destino indicado como indisponível até nova escolha.

Shared coordena alterações de conexão, ativação do retorno e reprodução A/B.
As transições usam identificadores e a coordenação de ciclo de vida já
existente. Resultados atrasados de listagem, abertura, cancelamento e saída
de processos não podem ativar uma sessão antiga nem alterar mensagens de
uma operação nova. O encerramento do app cancela todos os fluxos locais.

## Testes e critérios de aceitação

Para cada comportamento, primeiro escrever teste, observar falha esperada,
implementar o mínimo e verificar verde antes de refatorar.

- Listagem com JSON realista: saídas físicas/virtuais, nomes amigáveis iguais,
  campos ausentes, conteúdo inválido, exclusão de `vdo_input` e nodes internos.
- Saída desaparecida ou substituída, erro/timeout de listagem e resultado antigo.
- Ativação desconectada, sem escolha ou com escolha removida: recusar no backend.
- PCM tratado capturado corretamente e saída principal/gravações idênticas com
  retorno ligado e desligado; blocos de 480, 960 e 5760 amostras e sobra curta.
- Volume 0/25/100%, rampa de 5 ms, mudanças sucessivas e valores inválidos.
- Fila cheia com consumidor parado: produtor não espera e retorno termina com erro.
- Desligar, trocar destino, desconectar/reconectar e trocar sessão descartam
  blocos e resultados antigos; não há reprodução atrasada nem reativação automática.
- Processo recebe alvo explícito e PCM esperado; falhas de spawn/escrita/saída,
  cancelamento com pipe bloqueado e recolhimento do filho.
- Troca A/B ↔ retorno aguarda encerramento anterior sem bloquear GTK/pump;
  gravação simultânea preserva os dois buffers do teste.
- Estado visual e controles em GTK; layout em janela pequena e ampla.

Executar suíte completa, build, formatação e diff; revisão independente antes
do merge e suíte novamente na `main`. Testes automatizados devem usar processos
controlados sem depender de placa de som. Verificar separadamente o PipeWire
real disponível e relatar o que exigir validação auditiva no fone/celular.

## Fora desta branch

Indicadores dos efeitos, diagnóstico geral, perfis personalizados,
calibração, mute e indicadores de saturação continuam em branches futuras.
Não adicionar retorno no próprio celular nem cancelamento de eco nesta etapa.
