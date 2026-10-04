# Cookie de pareamento: só HTTPS, inacessível ao JS e nunca enviado entre sites.
Rails.application.config.session_store :cookie_store,
  key: "_micbridge", secure: true, httponly: true, same_site: :strict
