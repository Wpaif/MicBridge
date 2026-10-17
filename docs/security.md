# Segurança e dados locais

## Pareamento

O app gera um token de pareamento para o QR e aceita novos celulares apenas
durante a janela **Aceitar novo celular**. Na primeira utilização essa janela
abre por cinco minutos; um pareamento bem-sucedido a fecha. **Esquecer todos**
revoga as credenciais salvas e reabre o pareamento.

O navegador recebe identificador e segredo em cookie `Secure`, `HttpOnly` e
`SameSite=Strict`, com validade configurada de um ano. O PC guarda o hash do
segredo em `devices.json`, criado com permissão 0600; o diretório de pareamento
usa 0700. O token do QR muda entre execuções; um navegador já reconhecido usa
suas credenciais, sem depender do token antigo.

Apenas um celular é aceito por vez. Outro dispositivo recebe conflito enquanto
o atual está conectado ou no período de reconexão. As verificações estão em
[state.rs](../app/src/state.rs), [web.rs](../app/src/web.rs) e
[server.rs](../app/src/server.rs).

## Certificados

O MicBridge cria a **MicBridge Local CA** e usa essa CA para assinar o certificado
HTTPS do servidor. Ele inclui o IP configurado e `micbridge.local`. O certificado
do servidor é renovado conforme idade/IP, preservando a CA existente.

O certificado público da CA está disponível em `https://<endereço>:3443/ca.crt`.
Para evitar avisos recorrentes do navegador, instale essa CA no celular usando
o procedimento do sistema operacional. Antes de confiar numa CA, confirme que
ela pertence ao seu PC; a interface também exibe a impressão digital SHA-256 do
certificado do servidor para conferência no navegador.

O fluxo e a interface de instalação de CA variam entre sistemas e navegadores;
o app não instala certificados no celular automaticamente. A chave `ca.key`
permanece no PC e não é disponibilizada pela rota de download.

## Rede

HTTPS usa a porta 3443, vinculada ao IP local descoberto na inicialização e ao
loopback. O listener HTTP não usa `0.0.0.0`. A sinalização WebRTC usa esse servidor;
o transporte de mídia é negociado separadamente pelo WebRTC.

Não há STUN configurado: o uso previsto é na rede local. A página receber via
HTTPS não garante, por si só, que a mídia conseguirá atravessar isolamento entre
clientes Wi-Fi ou regras de firewall. Veja [Solução de problemas](troubleshooting.md).

As páginas entregues por `web.rs` usam CSP restrita, `Cache-Control: no-store`,
`X-Content-Type-Options: nosniff` e `Referrer-Policy: no-referrer`. Isso não substitui
as verificações de autenticação nas rotas de sinalização.

## Persistência e gravações

Os arquivos ficam em `$XDG_DATA_HOME/micbridge`, ou `~/.local/share/micbridge`.
Consulte o [mapa dos arquivos](architecture.md#arquivos-do-aplicativo).

O áudio é processado localmente. A gravação de teste fica em memória até a
exportação explícita de WAVs. Esses WAVs contêm voz original e tratada; o mute
da saída virtual não silencia a gravação nem o retorno.

Perfis e temas ficam em `settings.json`. O registro `monitor-routing.json` guarda
links de áudio necessários à restauração do retorno exclusivo. Não apague esse
registro para tentar corrigir uma restauração pendente; use o controle de retry.

Uma instalação comum preserva esses dados. `--reinstall` e `--purge` os removem
conforme as opções da [instalação](installation.md#reinstalar-ou-remover).
