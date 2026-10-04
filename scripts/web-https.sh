#!/usr/bin/env bash
# Sobe o Rails em HTTPS na LAN (porta 3443). O celular precisa de HTTPS para usar o microfone.
#
# TLS: uma CA local (MicBridge Local CA) assina o certificado do servidor. Instale web/tmp/certs/ca.pem
# no celular uma vez (também em https://<IP>:3443/ca.crt) para confiar sem avisos. A chave da CA
# nunca sai deste PC. O certificado é refeito automaticamente se o IP da máquina mudar.
set -euo pipefail
# Quando lançado pelo menu (sem shell de login), o Ruby do asdf não está no PATH.
export PATH="$HOME/.asdf/shims:$PATH"
cd "$(dirname "$0")/../web"
C=tmp/certs
mkdir -p "$C"; chmod 700 "$C"
IP=$(ip -4 route get 1.1.1.1 | grep -oP 'src \K\S+')

if [[ ! -f $C/ca.pem ]]; then
  openssl req -x509 -newkey rsa:3072 -nodes -days 3650 -keyout $C/ca.key -out $C/ca.pem \
    -subj "/CN=MicBridge Local CA" \
    -addext "basicConstraints=critical,CA:TRUE,pathlen:0" -addext "keyUsage=critical,keyCertSign,cRLSign"
  chmod 600 $C/ca.key
fi

if [[ ! -f $C/cert.pem ]] || ! openssl x509 -in $C/cert.pem -noout -ext subjectAltName | grep -q "IP Address:${IP}\b"; then
  openssl req -newkey rsa:2048 -nodes -keyout $C/key.pem -out $C/server.csr -subj "/CN=micbridge.local"
  openssl x509 -req -in $C/server.csr -CA $C/ca.pem -CAkey $C/ca.key -CAcreateserial -days 397 -out $C/cert.pem \
    -extfile <(printf "subjectAltName=IP:%s,DNS:micbridge.local\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n" "$IP")
  chmod 600 $C/key.pem
fi

# Escuta apenas na interface da LAN (e no loopback), não em todas.
exec bundle exec puma \
  -b "ssl://${IP}:3443?key=$C/key.pem&cert=$C/cert.pem" \
  -b "ssl://127.0.0.1:3443?key=$C/key.pem&cert=$C/cert.pem"
