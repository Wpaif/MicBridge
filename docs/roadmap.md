# Plano de evolução

Revisão em 09/11/2026. Tudo o que aparece como implementado está integrado à
`main`; ainda não há release publicada.

## Oito melhorias iniciais

| Etapa | Funcionalidade | Estado |
|---|---|---|
| 1 | Gravação de teste e comparação A/B | Implementada: captura original/tratada, reprodução e exportação WAV. |
| 2 | Retorno ao vivo no fone | Implementado: saída escolhida, volume próprio e ajuste exclusivo com restauração. |
| 3 | Indicadores de atuação dos efeitos | Implementados na mesa de efeitos: LED do gate (aberto, espera, fechado), medidor de redução do compressor (GR) e de ajuste do ganho automático. |
| 4 | Diagnóstico geral de áudio/conexão | Adiado por decisão do usuário. |
| 5 | Perfis personalizados de áudio | Implementados: criar, selecionar, atualizar, renomear e excluir, incluindo o equalizador; sem importação/exportação. |
| 6 | Calibração automática | Implementada: mede silêncio e leitura e sugere limiar do gate, redução de ruído e ganho manual; aplicar vira alteração não salva do perfil. |
| 7 | Mute | Implementado na saída virtual; retorno e gravação continuam ativos. |
| 8 | Indicadores dedicados de saturação/clipping | Implementados: **CLIP** nos medidores de entrada (celular saturado) e de saída (limitador cortando mais de 6 dB), aceso por 3 s. |

Sete das oito melhorias estão implementadas; só o diagnóstico (etapa 4) segue
adiado. Confirmação manual e integração de branches são etapas próprias; consulte
o histórico Git e os procedimentos em [Desenvolvimento](development.md#testes).

## Melhorias adicionais presentes

- Interface única em Quickshell, com trilho de seções, cards e atalhos de teclado.
  A interface GTK foi removida para concentrar o trabalho numa só.
- Mesa de som para os efeitos, na ordem do sinal, com rolagem horizontal quando
  não cabe na janela.
- Equalizador de 3 bandas (120 Hz, 1 kHz e 6 kHz, ±12 dB), salvo nos perfis.
- Compressor depois do ganho automático, com makeup automático: antes ele não
  atuava com a voz no nível que o celular manda.
- Fonte extra: o som de um app (como uma aba do navegador) junto com a voz.
- Testes de qualidade do DSP no CI com oito ruídos (branco, rosa, marrom, azul,
  violeta, zumbido de 60 Hz, teclado e vozes ao fundo): separação, nível final,
  teto do limitador, atuação do compressor e diferença entre perfis.
- Relatório com voz real (só local): separação por ruído, fala cortada por perfil
  e por efeito, e os trechos cortados num WAV para ouvir.
- Redução de ruído que devolve parte do original quando há voz, alinhada ao
  atraso do RNNoise, e fader de força em limite de atenuação.
- Atalhos do equalizador (Plano, Clarear, Encorpar, Secar), com teste que confere
  para onde cada um leva o timbre.
- Código sem avisos do clippy, com o CI recusando avisos novos.
- Cores que acompanham o tema do Omarchy.
- Proteção dos faders contra alterações acidentais ao rolar.
- Controles de retorno bloqueados durante preparação/restauração.

## Próximos passos

| Prioridade | Item | Observação |
|---|---|---|
| 1 | Validação de ouvido | Ouvir com o celular os perfis após a mudança do compressor, o EQ durante a fala, a fonte extra numa chamada real e o gate depois da calibração em ambientes diferentes. A bancada mediu, mas não substitui a escuta. |
| 2 | Analisador de espectro | Mostrar no canal EQ o que cada banda está fazendo. |
| 3 | Isolamento de voz | Avaliado; veja abaixo. Próximo passo, se retomado: comparar RNNoise, WebRTC e DeepFilterNet com o relatório de voz real antes de integrar. |
| — | Redução de ruído | Medida com oito ruídos e com voz real. Ela apagava sílabas fracas; o ajuste reduziu de 23% para 19% do quarto mais baixo da fala, e o resto é limite do RNNoise. Um fader de força troca ruído por fala preservada (em 25%, 15% de corte). Teclado (+12 dB) e vozes ao fundo (sem separação) são os pontos fracos. Faltam ruídos gravados de verdade. |

## Isolamento de voz (avaliação de 09/11/2026)

Vozes ao fundo são o ponto mais fraco: o tratamento não as separa (-1,7 a
-6,0 dB), porque RNNoise e gate tratam qualquer fala como fala.

| Opção | Resultado |
|---|---|
| Isolamento de Voz do iPhone (página em modo de chamada) | Testado e descartado: o som ficou pior que o microfone cru tratado no PC. |
| DeepFilterNet (Rust, MIT/Apache) | Candidato mais forte para ruído e sílabas fracas. Sem release desde 2023, fora do crates.io atualizado, latência e binário maiores. Deve manter vozes ao fundo como o RNNoise. |
| WebRTC Audio Processing, SpeexDSP, libspecbleach (C/C++) | Consolidadas, mas clássicas: boas com ruído constante, fracas com teclado e vozes. A WebRTC 2.0 removeu a supressão de transientes (teclado). |
| Isolamento da voz cadastrada (como Apple e Krisp) | Única abordagem que separa a sua voz de outras. Sem biblioteca aberta pronta conhecida; seria um projeto à parte. |

Novos itens devem ser descritos com comportamento, validação e limites claros
antes de aparecer como recursos disponíveis no README.
