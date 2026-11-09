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

Sete das oito melhorias estão implementadas; só o diagnóstico (etapa 4) segue adiado. Confirmação manual e integração de
branches são etapas próprias; consulte o histórico Git e os procedimentos em
[Desenvolvimento](development.md#testes).

## Melhorias adicionais presentes

- Interface única em Quickshell, com trilho de seções, cards e atalhos de teclado.
  A interface GTK foi removida para concentrar o trabalho numa só.
- Mesa de som para os efeitos, na ordem do sinal, com rolagem horizontal quando
  não cabe na janela.
- Equalizador de 3 bandas (120 Hz, 1 kHz e 6 kHz, ±12 dB), salvo nos perfis.
- Compressor depois do ganho automático, com makeup automático: antes ele não
  atuava com a voz no nível que o celular manda.
- Fonte extra: o som de um app (como uma aba do navegador) junto com a voz.
- Testes de qualidade do DSP no CI: separação entre voz e ruído, nível final,
  teto do limitador, atuação do compressor e diferença entre perfis.
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
| — | Redução de ruído com ruídos reais | Medida com ruído rosa: +20 a +31 dB de separação. Falta avaliar com teclado, rua e ventilador. |

Novos itens devem ser descritos com comportamento, validação e limites claros
antes de aparecer como recursos disponíveis no README.
