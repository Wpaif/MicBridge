# Gravação de teste e comparação A/B

## Objetivo e organização

Primeira das oito funcionalidades solicitadas. Branch: `feat/test-recording`.
Cada recurso terá sua própria branch, testes pelo ciclo vermelho–verde–refatoração,
revisão e merge na `main` apenas depois da validação. Implementação sequencial:
este recurso deve estar integrado antes de iniciar o próximo.

As alterações de DSP e interface existentes na árvore de trabalho devem ser
preservadas e consolidadas separadamente da implementação deste recurso.

## Comportamento

Um painel “Teste de voz” oferece iniciar, parar, reproduzir original,
reproduzir tratado, parar reprodução e exportar. A captura é explícita,
limitada a 30 segundos a 48 kHz mono e termina automaticamente nesse limite.
O tempo exibido representa amostras capturadas, não tempo de relógio.

“Original” significa PCM decodificado do Opus antes dos efeitos locais;
“Tratado” significa o resultado após o DSP, antes da saída PipeWire.
Os dois buffers mantêm correspondência exata de amostras. Não se promete
compensar o atraso interno do RNNoise nesta primeira implementação.

Iniciar uma nova captura substitui o teste anterior e encerra sua reprodução.
Parar preserva o trecho recebido. Uma desconexão finaliza a captura parcial;
reconectar não continua nem mistura outra sessão no mesmo teste.
Sem amostras, reprodução e exportação ficam indisponíveis.

A reprodução é exclusiva: original ou tratado, nunca simultâneos. Usa o
reprodutor PipeWire no dispositivo padrão de saída do sistema, sem direcionar
ao microfone virtual. Falhas aparecem no painel e permitem tentar novamente.
Seleção de dispositivo pertence à futura funcionalidade de retorno no fone.

Exportar abre o seletor de destino e produz dois WAV PCM de 16 bits mono,
48 kHz, identificados como original e tratado. Cancelar não grava arquivos.
Sobrescrita exige confirmação no seletor. Valores fora da faixa são saturados
na conversão para WAV; valores não finitos viram zero. Erros de escrita
são informados, incluindo eventual exportação parcial de apenas um dos arquivos.

## Arquitetura

Módulo próprio para captura, estado do teste e codificação WAV. Captura usa
fila limitada de pares de blocos antes/depois do DSP, consumida fora do caminho
principal. O produtor não espera por disco, reprodução ou pelo consumidor.

Se a fila lotar, encerra o teste como incompleto e avisa; não deixa lacunas
silenciosas ou canais de comprimentos diferentes. A escrita dos arquivos e a
reprodução ocorrem fora da thread GTK. Os buffers são limitados a 1.440.000
amostras por versão, aproximadamente 11,52 MB no total em f32, além da fila.

Gravação e reprodução são locais. Nenhum arquivo é salvo automaticamente.
O estado usa identificadores de captura e de conexão para rejeitar blocos e
resultados atrasados de operações anteriores. A reprodução deve ser encerrada
ao fechar o aplicativo ou substituir o teste.

## Critérios de aceitação e TDD

Antes de cada comportamento, escrever o teste e observar a falha esperada;
implementar o mínimo, executar novamente e refatorar com testes verdes.

Testar pelo menos:

- Identidade e ordem das amostras original/tratado com blocos conhecidos.
- Limite de 30 segundos, inclusive bloco que cruza a fronteira.
- Parada explícita, parada sem áudio e nova captura sem dados anteriores.
- Desconexão, reconexão e blocos atrasados de uma captura substituída.
- Fila cheia sem bloqueio do produtor e com aviso de teste incompleto.
- Cabeçalho, taxa, número de canais, comprimento e amostras WAV decodificadas
  por leitura independente; saturação e valores não finitos.
- Cancelamento da exportação e erros de escrita.
- Exclusão mútua da reprodução, parada e falha do processo reprodutor.
- Captura dos dois lados do DSP real, sem alterar o áudio entregue à saída.

Executar a suíte completa com `cargo test --offline`, verificar formatação e
compilação, revisar o diff e verificar o painel em janela pequena e ampla.
Registrar separadamente os testes automatizados e as verificações que exigem
celular, áudio real e dispositivo de reprodução. Merge somente com os testes
passando; uma limitação ambiental deve ser relatada, nunca tratada como teste aprovado.

## Fora desta branch

Retorno ao vivo, indicadores dos efeitos, diagnóstico geral, perfis,
calibração, mute e indicação de saturação terão branches próprias.
