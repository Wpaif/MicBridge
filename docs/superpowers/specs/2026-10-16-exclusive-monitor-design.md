# Retorno exclusivo para ajuste em tempo real

## Objetivo e escopo

Ao ativar o retorno, ouvir apenas a voz tratada do celular na saída selecionada,
seguindo a lógica dos cabos do qpwgraph. Outros aplicativos que estejam ligados
àquela saída ficam temporariamente desconectados dela. Desativar encerra o retorno
e tenta restaurar as conexões removidas. O diagnóstico geral permanece adiado.

A solicitação autoriza alterar os links da saída escolhida durante o ajuste.
Restaurar os links removidos é uma decisão proposta para tornar esse modo temporário.
A implementação permanece na branch `fix/bluetooth-monitor`, com TDD e revisão
antes do merge local já autorizado. Preservar o teste Bluetooth existente.

## Experiência de uso

1. Conectar o celular e escolher o fone/saída no painel de retorno.
2. Ativar “Ajuste em tempo real · retorno exclusivo”. O painel explica previamente:
   “Durante o ajuste, outros aplicativos ficam desconectados desta saída”.
3. Mostrar “Preparando saída exclusiva…” enquanto valida o dispositivo, abre o
   fluxo de retorno e organiza as conexões. Só mostrar ativo depois da confirmação
   dos links corretos e da ausência de outros links para essa saída.
4. A voz tratada chega continuamente ao fone; mudanças dos efeitos são ouvidas
   pelo mesmo caminho. O volume local continua independente, inicialmente 25%.
5. Desligar mostra “Restaurando conexões…” até concluir a tentativa de restauração.
   Falhas aparecem no painel com ação de tentar restaurar novamente.

O modo exige áudio do celular, como o retorno atual. Não captura o microfone do
fone/computador e não mede latência total. O atraso do Bluetooth continua existindo.
Não apresentar essa mudança como prova de que a falha de voz relatada já foi resolvida:
validar a entrega de áudio e o funcionamento real separadamente.

## Abordagem escolhida

Controlar links reais do PipeWire, consultados por `pw-dump` e manipulados por
`pw-link`. O qpwgraph não precisa estar instalado ou aberto.

Alternativas: silenciar os aplicativos manteria os cabos conectados e não cumpriria
a solicitação; pedir ligação manual no qpwgraph transferiria ao usuário a operação
que ele quer automatizar. O controle dos links atende diretamente ao fluxo pedido.

## Regras de roteamento

- O limite da operação é o node de saída explicitamente escolhido, validado por
  ID, nome e serial. Não tocar em outras saídas, fontes ou no microfone virtual.
- Identificar o fluxo desta sessão de retorno por um nome único e sua identidade
  PipeWire. Não aceitar outro fluxo só por ter nome semelhante.
- Antes de remover links alheios, confirmar que o retorno está conectado ao destino
  correto. Manter o pareamento de canais compatível com as portas do dispositivo;
  não inventar ligações quando os canais não forem identificáveis.
- Guardar as identidades dos nodes e portas, direções, canais e propriedades
  necessárias dos links removidos. Um ID numérico reutilizado não autoriza ações
  sobre um novo objeto. Na restauração, criar somente links ainda ausentes entre
  os mesmos objetos que continuam disponíveis.
- Enquanto ativo, observar mudanças do grafo: links de outros aplicativos que
  apareçam na saída escolhida também são guardados e removidos. Nunca remover os
  links pertencentes ao próprio retorno.
- Se a exclusividade não puder ser estabelecida ou mantida, interromper o modo,
  explicar a falha e iniciar a restauração. Não afirmar exclusividade parcial.
- Retirada/substituição do fone interrompe o ajuste. Não trocar para alto-falantes
  ou para outro dispositivo de mesmo nome. Não religar automaticamente.
- Troca de saída, reprodução A/B, desconexão do celular e encerramento normal passam
  pelo encerramento/restauração da sessão anterior antes de iniciar outro modo.
- Uma reconexão do celular ou comando novo invalida operações antigas, inclusive
  consultas, ligações e publicações de estado atrasadas.

## Restauração e falhas

Restaurar os links removidos que ainda forem válidos; não restaurar objetos que
sumiram, foram substituídos ou já estão ligados. Não desfazer alterações em outras
saídas feitas pelo usuário. Registrar separadamente links restaurados, já existentes,
indisponíveis e com erro. Uma falha de restauração permanece visível e permite nova
tentativa; não apagar o registro antes de confirmar o resultado.

Manter um registro local das conexões removidas antes de cada remoção para permitir
recuperação na próxima abertura caso o processo termine abruptamente. Recuperar
somente endpoints cuja identidade continue válida; não reutilizar IDs antigos de
outra sessão do PipeWire. Encerramento forçado pode deixar as conexões interrompidas
até a recuperação na próxima execução ou ajuste manual no qpwgraph.

Falhas parciais na ativação também exigem rollback. O resultado da execução de
`pw-link` deve ser confirmado por nova leitura do grafo; código de saída sozinho
não prova que o estado desejado permaneceu estabelecido.

## Integração

Adicionar controle de roteamento ao ciclo de vida do retorno e às confirmações de
parada já existentes. O controle usa um worker próprio, sem segurar o pump ou a
thread GTK enquanto consulta/manipula o grafo. A escrita PCM continua independente
para que consultas lentas não provoquem sobrecarga artificial da fila de retorno.

A fila e o tratamento do áudio principal permanecem com seus contratos atuais.
Falhas de roteamento aparecem no painel local. Identificar como sessão antiga toda
resposta que chegue depois do cancelamento. A confirmação de parada inclui término
do reprodutor e resultado da restauração; não liberar outro modo antes disso.

## Validação exigida

Testes com grafo/processos controlados, primeiro RED e depois GREEN:

- saída com dois aplicativos: somente os links desses aplicativos para a saída
  escolhida são removidos; os do retorno e de outros destinos permanecem;
- restauração idempotente, canais corretos, links já existentes e endpoints removidos;
- reutilização de IDs/nomes com novo serial nunca liga ou desliga outro dispositivo;
- link concorrente novo durante o ajuste é isolado e posteriormente restaurado;
- erro de ligação/desligamento, timeout e rollback parcial são apresentados corretamente;
- desligamento/troca de saída/A/B/desconexão durante ativação e respostas antigas;
- recuperação do registro após interrupção e recusa de dados de outra sessão PipeWire;
- consultas lentas não bloqueiam o produtor de áudio nem a interface;
- PCM principal e A/B continuam idênticos, além dos testes existentes de retorno;
- teste GTK separado para estados de preparação, exclusividade e restauração.

Verificação real: no Bluetooth conectado, registrar grafo antes/durante/depois,
confirmar fluxo PCM não silencioso no destino e comparar a restauração dos links.
Relatar separadamente a escuta de fala do celular, que exige participação do usuário.
Executar suíte, build, formatação, revisão independente e suíte após o merge.
