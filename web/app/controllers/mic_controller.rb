require "net/http"

# Serve a página do celular e repassa o SDP offer WebRTC ao app Rust (WHIP).
# O Rails existe porque getUserMedia exige HTTPS; o Rust fica só em localhost.
#
# Segurança: o app gera um token por sessão (MICBRIDGE_TOKEN) embutido no QR code.
# Quem abre o link com o token recebe um cookie de sessão assinado; sem ele, nada funciona.
class MicController < ApplicationController
  skip_forgery_protection only: :offer
  before_action :pair_with_token, only: :show
  before_action :require_pairing, except: :ca

  CORE_URL = URI(ENV.fetch("MICBRIDGE_CORE_URL", "http://127.0.0.1:7878/whip"))

  def show; end

  # Certificado público da CA local (não é segredo): instale no celular para confiar no servidor.
  def ca
    send_file Rails.root.join("tmp/certs/ca.pem"), type: "application/x-x509-ca-cert",
                                                   disposition: "attachment", filename: "micbridge-ca.crt"
  end

  def offer
    res = core_post("whip", session[:device], request.raw_post, "application/sdp")
    render plain: res.body, status: res.code.to_i, content_type: "application/sdp"
  rescue SystemCallError, Net::OpenTimeout, Net::ReadTimeout
    render plain: "App MicBridge não está rodando no PC", status: :bad_gateway
  end

  private

  def expected_token
    ENV["MICBRIDGE_TOKEN"].to_s
  end

  def valid_token?(given)
    expected = expected_token
    expected.present? && given.is_a?(String) &&
      ActiveSupport::SecurityUtils.secure_compare(given, expected)
  end

  # Link do QR code (?t=...): troca o token por cookie de sessão e limpa a URL.
  def pair_with_token
    return unless params[:t].present?
    return render_denied unless valid_token?(params[:t])

    device = session[:device] || SecureRandom.hex(16)
    case core_post("pair", device).code.to_i
    when 200, 201
      session[:paired_token] = params[:t]
      session[:device] = device
      redirect_to root_path
    when 403
      render_denied("Pareamento fechado. Ative \"Aceitar novo celular\" no app do PC.")
    else
      render_denied
    end
  rescue SystemCallError, Net::OpenTimeout, Net::ReadTimeout
    render plain: "App MicBridge não está rodando no PC", status: :bad_gateway
  end

  def core_post(path, device, body = "", type = "text/plain")
    uri = URI.join(CORE_URL.to_s, path)
    Net::HTTP.start(uri.host, uri.port, open_timeout: 2, read_timeout: 5) do |http|
      http.post(uri.path, body, "Content-Type" => type,
                "X-MicBridge-Token" => expected_token, "X-MicBridge-Device" => device)
    end
  end

  def require_pairing
    render_denied unless valid_token?(session[:paired_token]) && session[:device].present?
  end

  def render_denied(message = "Acesso não autorizado.<br>Escaneie o QR code mostrado no app do PC.")
    @message = message
    render "mic/denied", status: :forbidden
  end
end
