require "net/http"

# Serve a página do celular e repassa as chamadas ao app Rust (localhost).
# O Rails existe porque getUserMedia exige HTTPS.
#
# Segurança/pareamento:
# - O QR code traz um token (?t=) válido só com a janela de pareamento aberta no app.
# - Ao parear, o app devolve id + segredo de longa duração, guardados no cookie de sessão
#   (1 ano). Depois disso o celular é reconhecido sem novo QR, mesmo após reiniciar o app.
# - O Rails fala com o app com um token interno (MICBRIDGE_TOKEN) que o celular nunca vê.
class MicController < ApplicationController
  skip_forgery_protection only: %i[offer stop]
  before_action :pair_with_token, only: :show
  before_action :require_pairing, only: :show
  before_action :require_credentials, only: %i[offer stop]

  CORE_URL = URI(ENV.fetch("MICBRIDGE_CORE_URL", "http://127.0.0.1:7878/whip"))

  rescue_from SystemCallError, Net::OpenTimeout, Net::ReadTimeout do
    render plain: "App MicBridge não está rodando no PC", status: :bad_gateway
  end

  def show; end

  # Certificado público da CA local (não é segredo): instale no celular para confiar no servidor.
  def ca
    send_file Rails.root.join("tmp/certs/ca.pem"), type: "application/x-x509-ca-cert",
                                                   disposition: "attachment", filename: "micbridge-ca.crt"
  end

  def offer
    res = core(Net::HTTP::Post, "whip", body: request.raw_post, type: "application/sdp")
    render plain: res.body, status: res.code.to_i, content_type: "application/sdp"
  end

  def stop
    core(Net::HTTP::Delete, "whip")
    head :no_content
  end

  private

  def core(klass, path, body: nil, type: "text/plain", extra: {})
    uri = URI.join(CORE_URL.to_s, path)
    req = klass.new(uri.path)
    req["Content-Type"] = type
    req["X-MicBridge-Token"] = ENV["MICBRIDGE_TOKEN"].to_s
    req["X-MicBridge-Device"] = session[:device].to_s
    req["X-MicBridge-Secret"] = session[:secret].to_s
    extra.each { |k, v| req[k] = v }
    req.body = body if body
    Net::HTTP.start(uri.host, uri.port, open_timeout: 2, read_timeout: 5) { |http| http.request(req) }
  end

  # Link do QR code (?t=...): troca o token por credenciais permanentes e limpa a URL.
  def pair_with_token
    return unless params[:t].present?

    res = core(Net::HTTP::Post, "pair", extra: { "X-MicBridge-Pair" => params[:t].to_s })
    case res.code.to_i
    when 201
      id, secret = res.body.split("\n", 2)
      session[:device] = id
      session[:secret] = secret
      redirect_to root_path
    when 403
      render_denied("Pareamento fechado. Ative \"Aceitar novo celular\" no app do PC.")
    else
      render_denied
    end
  end

  def require_credentials
    head :forbidden unless session[:device].present? && session[:secret].present?
  end

  # A página só abre para celular já pareado; o app confirma as credenciais.
  def require_pairing
    return render_denied unless session[:device].present? && session[:secret].present?

    if core(Net::HTTP::Post, "verify").code.to_i != 200
      reset_session
      render_denied("Este celular não está pareado. Escaneie o QR code mostrado no app do PC.")
    end
  end

  def render_denied(message = "Acesso não autorizado.<br>Escaneie o QR code mostrado no app do PC.")
    @message = message
    render "mic/denied", status: :forbidden
  end
end
