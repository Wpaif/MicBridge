require "net/http"

# Serve a página do celular e repassa o SDP offer WebRTC ao app Rust (WHIP).
# O Rails existe porque getUserMedia exige HTTPS; o Rust fica só em localhost.
class MicController < ApplicationController
  skip_forgery_protection only: :offer

  CORE_URL = URI(ENV.fetch("MICBRIDGE_CORE_URL", "http://127.0.0.1:7878/whip"))

  def show; end

  def offer
    res = Net::HTTP.start(CORE_URL.host, CORE_URL.port, open_timeout: 2, read_timeout: 5) do |http|
      http.post(CORE_URL.path, request.raw_post, "Content-Type" => "application/sdp")
    end
    render plain: res.body, status: res.code.to_i, content_type: "application/sdp"
  rescue SystemCallError, Net::OpenTimeout, Net::ReadTimeout
    render plain: "App MicBridge não está rodando no PC", status: :bad_gateway
  end
end
