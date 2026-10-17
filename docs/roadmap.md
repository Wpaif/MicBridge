# Plano de evolução

Revisão em 17/10/2026, considerando o código disponível nesta linha de
desenvolvimento. O estado abaixo não afirma que as branches já foram integradas
à `main` ou publicadas como release.

## Oito melhorias iniciais

| Etapa | Funcionalidade | Estado |
|---|---|---|
| 1 | Gravação de teste e comparação A/B | Implementada: captura original/tratada, reprodução e exportação WAV. |
| 2 | Retorno ao vivo no fone | Implementado: saída escolhida, volume próprio e ajuste exclusivo com restauração. |
| 3 | Indicadores de atuação dos efeitos | Implementados para gate, compressor e volume automático. |
| 4 | Diagnóstico geral de áudio/conexão | Adiado por decisão do usuário. |
| 5 | Perfis personalizados de áudio | Implementados: criar, selecionar, atualizar, renomear e excluir; sem importação/exportação. |
| 6 | Calibração automática | Pendente. |
| 7 | Mute | Implementado na saída virtual; retorno e gravação continuam ativos. |
| 8 | Indicadores dedicados de saturação/clipping | Pendentes. Os medidores atuais não substituem esse recurso. |

Cinco das oito melhorias estão implementadas nesta linha. Confirmação manual e
integração de branches são etapas próprias; consulte o histórico Git e os
procedimentos em [Desenvolvimento](development.md#testes).

## Melhorias adicionais presentes

- Layout com status fixo, lateral de conexão e estúdio responsivo.
- Temas visuais, transparência e paletas JSON com importação/exportação.
- Detecção de Omarchy para acompanhar a paleta no modo sistema.
- Proteção dos deslizantes contra alterações acidentais ao rolar.
- Controles de retorno bloqueados durante preparação/restauração.

Novos itens devem ser descritos com comportamento, validação e limites claros
antes de aparecer como recursos disponíveis no README.
