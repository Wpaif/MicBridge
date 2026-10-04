require "net/http"

# Serve a página do celular e repassa o SDP offer WebRTC ao app Rust (WHIP).
# O Rails existe porque getUserMedia exige HTTPS; o Rust fica só em localhost.
#
# Segurança: o app gera um token por sessão (MICBRIDGE_TOKEN) embutido no QR code.
# Quem abre o link com o token recebe um cookie de sessão assinado; sem ele, nada funciona.
class MicController < ApplicationController
  skip_forgery_protection only: :offer
  before_action :pair_with_token, only: :show
  before_action :require_pairing

  CORE_URL = URI(ENV.fetch("MICBRIDGE_CORE_URL", "http://127.0.0.1:7878/whip"))

  def show; end

  def offer
    res = Net::HTTP.start(CORE_URL.host, CORE_URL.port, open_timeout: 2, read_timeout: 5) do |http|
      http.post(CORE_URL.path, request.raw_post,
                "Content-Type" => "application/sdp",
                "X-MicBridge-Token" => expected_token,
                "X-MicBridge-Device" => session[:device])
    end
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
    if valid_token?(params[:t])
      session[:paired_token] = params[:t]
      session[:device] ||= SecureRandom.hex(16)
      redirect_to root_path
    else
      render_denied
    end
  end

  def require_pairing
    render_denied unless valid_token?(session[:paired_token]) && session[:device].present?
  end

  def render_denied
    render "mic/denied", status: :forbidden
  end
end
