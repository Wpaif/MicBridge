import { Controller } from "@hotwired/stimulus"

// Captura o microfone SEM processamento do navegador (o tratamento é feito no PC)
// e envia via WebRTC (Opus) ao app Rust.
export default class extends Controller {
  static targets = ["status", "button", "meter"]

  async toggle() {
    this.pc ? this.stop() : await this.start()
  }

  async start() {
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
          channelCount: 1,
          sampleRate: 48000,
        },
      })
    } catch (e) {
      return this.setStatus(`Microfone negado: ${e.message}`)
    }

    this.pc = new RTCPeerConnection({ iceServers: [] }) // só rede local
    this.pc.onconnectionstatechange = () => {
      this.setStatus(this.pc?.connectionState ?? "")
      if (["failed", "closed"].includes(this.pc?.connectionState)) this.stop()
    }
    const track = this.stream.getAudioTracks()[0]
    const sender = this.pc.addTrack(track, this.stream)
    this.tuneSender(sender)

    await this.pc.setLocalDescription(await this.pc.createOffer())
    await this.waitIce()

    const res = await fetch("/offer", {
      method: "POST",
      headers: { "Content-Type": "application/sdp" },
      body: this.pc.localDescription.sdp,
    })
    if (!res.ok) {
      this.setStatus(await res.text())
      return this.stop()
    }
    await this.pc.setRemoteDescription({ type: "answer", sdp: await res.text() })
    this.buttonTarget.textContent = "Parar"
    this.startMeter()
  }

  // Opus de alta qualidade para voz, sem DTX; prioridade alta de rede.
  tuneSender(sender) {
    const p = sender.getParameters()
    p.encodings = [{ ...(p.encodings?.[0] ?? {}), maxBitrate: 64000, networkPriority: "high" }]
    sender.setParameters(p).catch(() => {})
  }

  waitIce() {
    if (this.pc.iceGatheringState === "complete") return
    return new Promise((resolve) => {
      this.pc.onicegatheringstatechange = () =>
        this.pc.iceGatheringState === "complete" && resolve()
      setTimeout(resolve, 2000)
    })
  }

  startMeter() {
    const ctx = new AudioContext()
    const analyser = ctx.createAnalyser()
    analyser.fftSize = 512
    ctx.createMediaStreamSource(this.stream).connect(analyser)
    const buf = new Float32Array(analyser.fftSize)
    const g = this.meterTarget.getContext("2d")
    const draw = () => {
      analyser.getFloatTimeDomainData(buf)
      const rms = Math.sqrt(buf.reduce((a, v) => a + v * v, 0) / buf.length)
      g.clearRect(0, 0, 300, 14)
      g.fillStyle = "#3584e4"
      g.fillRect(0, 0, Math.min(1, rms * 4) * 300, 14)
      this.raf = requestAnimationFrame(draw)
    }
    this.audioCtx = ctx
    draw()
  }

  stop() {
    cancelAnimationFrame(this.raf)
    this.audioCtx?.close()
    this.stream?.getTracks().forEach((t) => t.stop())
    this.pc?.close()
    this.pc = this.stream = this.audioCtx = null
    this.buttonTarget.textContent = "Iniciar microfone"
    this.setStatus("Desconectado")
  }

  setStatus(t) {
    this.statusTarget.textContent = t
  }

  disconnect() {
    this.stop()
  }
}
