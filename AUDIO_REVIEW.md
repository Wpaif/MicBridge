# Avaliação dos cortes de voz

## Falha reproduzida e corrigida

O noise gate consumia o hold de 200 ms nas pausas, mas não o renovava quando
a voz voltava à faixa de histerese. Assim, pausas separadas eram acumuladas.
Depois de esgotar o hold, um trecho baixo era atenuado e só uma fala acima do
limiar de abertura conseguia reabrir o gate.

Reprodução isolada, sem rede, GTK ou PipeWire:

- Gate com abertura em -40 dBFS e fechamento abaixo de -44 dBFS.
- Sinal inicial acima da abertura, seguido de trechos em aproximadamente
  -43 dBFS intercalados com pausas de 100 ms.
- Antes da correção, após a terceira pausa o RMS do primeiro frame de fala
  caía de aproximadamente 0,00707 para 0,00207 (cerca de 11 dB de perda).
- A correção renova o hold quando um gate já aberto recebe sinal na faixa
  de sustentação. O tempo de fechamento passa a exigir uma pausa contínua.

Testes novos em `app/src/dsp.rs` verificam a continuidade da fala baixa,
o fechamento após pausa longa e a reabertura com voz, e que ruído na faixa
de histerese não abre um gate fechado. O teste de continuidade falhou antes
da correção e passou depois. Os sinais sintéticos isolam o gate; não medem
a inteligibilidade da fala nem a qualidade do RNNoise.

Execute com:

```sh
cd app
cargo test --offline
```

## Outros pontos encontrados na leitura

- O gate mede o sinal **depois do RNNoise**. Voz baixa já atenuada pelo
  denoiser pode cair abaixo do limiar do gate. O perfil Streaming usa
  -45 dBFS, mais agressivo que os demais. Não foi demonstrado que isso
  explica o relato; alterar os limiares sem gravações também pode deixar
  mais ruído passar.
- `emit`, em `server.rs`, processa apenas blocos completos de 480 amostras
  e envia qualquer sobra sem tratamento. Se o decoder entregar blocos
  menores que 10 ms ou não múltiplos desse tamanho, haverá mistura de
  áudio tratado e bruto. Falta confirmar se isso ocorre na captura real.
- O processamento e a escrita no PipeWire são sequenciais no pump.
  Uma escrita bloqueada pode esperar até 250 ms; uma falha causa descarte
  e tentativas de reabertura com intervalo de um segundo. São caminhos
  capazes de interromper a saída, mas não foram reproduzidos nesta avaliação.

## Validação com voz real

Após recompilar, comparar a mesma frase com trechos baixos e pausas curtas,
mantendo celular, distância e aplicativo de destino. Comparar o perfil
habitual com gate desligado e depois com RNNoise desligado, um de cada vez.
Gravar a fonte Microfone_VDO ajuda a distinguir o processamento local de
eventual supressão adicional no aplicativo de chamada.

Esta avaliação confirma uma falha do gate, mas não prova que ela seja a
única causa dos cortes relatados. Não houve teste com celular, fala real
ou reprodução no PipeWire.
