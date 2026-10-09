# Cookie de pareamento: só HTTPS, inacessível ao JS, nunca enviado entre sites e válido por
# 1 ano, para o celular ser reconhecido sem novo QR.
Rails.application.config.session_store :cookie_store,
  key: "_micbridge", secure: true, httponly: true, same_site: :strict, expire_after: 1.year
