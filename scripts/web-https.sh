#!/usr/bin/env bash
# Sobe o Rails em HTTPS na LAN (porta 3443). O celular precisa de HTTPS para usar o microfone.
set -euo pipefail
# Quando lançado pelo menu (sem shell de login), o Ruby do asdf não está no PATH.
export PATH="$HOME/.asdf/shims:$PATH"
cd "$(dirname "$0")/../web"
mkdir -p tmp/certs
if [[ ! -f tmp/certs/key.pem ]]; then
  IP=$(ip -4 route get 1.1.1.1 | grep -oP 'src \K\S+')
  openssl req -x509 -newkey rsa:2048 -nodes -days 825 -keyout tmp/certs/key.pem -out tmp/certs/cert.pem \
    -subj "/CN=micbridge.local" -addext "subjectAltName=IP:${IP},DNS:micbridge.local"
fi
exec bundle exec puma -b "ssl://0.0.0.0:3443?key=tmp/certs/key.pem&cert=tmp/certs/cert.pem"
