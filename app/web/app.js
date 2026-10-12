// Captura o microfone SEM processamento do navegador (o tratamento é feito no PC) e envia via
// WebRTC (Opus) ao app. Reconecta sozinho se a rede cair ou a conexão falhar, sem novo QR.
const BACKOFF_MS = [1000, 2000, 3000, 5000, 8000, 10000]
const KEY = "micbridge.on"

class Mic {
  constructor() {
    this.button = document.getElementById("button")
    this.statusEl = document.getElementById("status")
    this.meter = document.getElementById("meter")
    this.badge = document.getElementById("badge")
    this.button.addEventListener("click", () => this.toggle())
    this.wanted = false
    this.retries = 0
    this.onVisible = () => document.visibilityState === "visible" && this.recoverNow()
    document.addEventListener("visibilitychange", this.onVisible)
    window.addEventListener("online", this.onVisible)
    // Estava ligado antes (página recarregada, celular desbloqueado)? Volta sozinho.
    if (localStorage.getItem(KEY) === "1") this.start()
  }

  toggle() {
    this.wanted ? this.stop() : this.start()
  }

  async start() {
    this.wanted = true
    localStorage.setItem(KEY, "1")
    this.button.textContent = "Parar"
    this.render("connecting", "Conectando…")
    if (!(await this.acquireMic())) return
    this.requestWakeLock()
    this.connectPeer()
  }

  // Parar de propósito: avisa o PC para mostrar "desconectado" na hora.
  stop() {
    this.wanted = false
    localStorage.removeItem(KEY)
    fetch("/offer", { method: "DELETE", credentials: "same-origin", keepalive: true }).catch(() => {})
    this.teardown()
    this.button.textContent = "Iniciar microfone"
    this.render("idle", "Desconectado")
  }

  async acquireMic() {
    if (this.stream?.getAudioTracks().some((t) => t.readyState === "live")) return true
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1, sampleRate: 48000 },
      })
      this.stream.getAudioTracks()[0].addEventListener("ended", () => this.wanted && this.recoverNow())
      return true
    } catch (e) {
      this.wanted = false
      localStorage.removeItem(KEY)
      this.button.textContent = "Iniciar microfone"
      this.render("error", `Microfone negado: ${e.message}`)
      return false
    }
  }

  async connectPeer() {
    this.closePeer()
    const pc = new RTCPeerConnection({ iceServers: [] }) // só rede local
    this.pc = pc
    pc.onconnectionstatechange = () => this.onState(pc)
    const sender = pc.addTrack(this.stream.getAudioTracks()[0], this.stream)
    this.tuneSender(sender)

    try {
      await pc.setLocalDescription(await pc.createOffer())
      await this.waitIce(pc)
      const res = await fetch("/offer", { method: "POST", credentials: "same-origin", headers: { "Content-Type": "application/sdp" }, body: pc.localDescription.sdp })
      if (pc !== this.pc) return // já foi substituída por outra tentativa
      if (res.status === 409) return this.fatal("Já existe outro celular conectado")
      if (res.status === 403) return this.fatal("Celular não pareado. Escaneie o QR code no PC.")
      if (!res.ok) throw new Error(await res.text())
      await pc.setRemoteDescription({ type: "answer", sdp: await res.text() })
    } catch (e) {
      if (pc === this.pc) this.scheduleRetry(`Sem conexão com o PC (${e.message})`)
    }
  }

  onState(pc) {
    if (pc !== this.pc || !this.wanted) return
    switch (pc.connectionState) {
      case "connected":
        this.retries = 0
        this.render("connected", "Conectado")
        this.startMeter()
        break
      case "disconnected": // pode se recuperar sozinha; dá 3 s antes de refazer
        this.render("reconnecting", "Conexão instável…")
        clearTimeout(this.graceTimer)
        this.graceTimer = setTimeout(() => pc === this.pc && pc.connectionState !== "connected" && this.scheduleRetry("Reconectando…"), 3000)
        break
      case "failed":
      case "closed":
        this.scheduleRetry("Reconectando…")
        break
    }
  }

  scheduleRetry(message) {
    if (!this.wanted) return
    clearTimeout(this.retryTimer)
    const delay = BACKOFF_MS[Math.min(this.retries++, BACKOFF_MS.length - 1)]
    this.render("reconnecting", message)
    this.retryTimer = setTimeout(async () => {
      if (!this.wanted) return
      if (await this.acquireMic()) this.connectPeer()
    }, delay)
  }

  // Tela desbloqueada / rede voltou: tenta já, sem esperar o backoff.
  recoverNow() {
    if (!this.wanted || this.pc?.connectionState === "connected") return
    clearTimeout(this.retryTimer)
    this.requestWakeLock()
    this.retries = 0
    this.acquireMic().then((ok) => ok && this.connectPeer())
  }

  fatal(message) {
    this.wanted = false
    localStorage.removeItem(KEY)
    this.teardown()
    this.button.textContent = "Iniciar microfone"
    this.render("error", message)
  }

  // Mantém a tela ligada enquanto transmite: navegadores suspendem o microfone ao bloquear.
  async requestWakeLock() {
    try {
      if ("wakeLock" in navigator && !this.wakeLock) {
        this.wakeLock = await navigator.wakeLock.request("screen")
        this.wakeLock.addEventListener("release", () => (this.wakeLock = null))
      }
    } catch (_) {}
  }

  // Opus de alta qualidade para voz; prioridade alta de rede.
  tuneSender(sender) {
    const p = sender.getParameters()
    p.encodings = [{ ...(p.encodings?.[0] ?? {}), maxBitrate: 64000, networkPriority: "high" }]
    sender.setParameters(p).catch(() => {})
  }

  waitIce(pc) {
    if (pc.iceGatheringState === "complete") return
    return new Promise((resolve) => {
      pc.onicegatheringstatechange = () => pc.iceGatheringState === "complete" && resolve()
      setTimeout(resolve, 2000)
    })
  }

  startMeter() {
    if (this.audioCtx || !this.stream) return
    try {
      const ctx = new AudioContext()
      const analyser = ctx.createAnalyser()
      analyser.fftSize = 512
      ctx.createMediaStreamSource(this.stream).connect(analyser)
      const buf = new Float32Array(analyser.fftSize)
      const g = this.meter.getContext("2d")
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
    } catch (_) {} // o medidor é opcional
  }

  closePeer() {
    clearTimeout(this.graceTimer)
    const pc = this.pc
    this.pc = null
    pc?.close()
  }

  teardown() {
    clearTimeout(this.retryTimer)
    cancelAnimationFrame(this.raf)
    this.audioCtx?.close().catch(() => {})
    this.audioCtx = null
    this.closePeer()
    this.stream?.getTracks().forEach((t) => t.stop())
    this.stream = null
    this.wakeLock?.release().catch(() => {})
    this.wakeLock = null
  }

  render(state, text) {
    this.badge.dataset.state = state
    this.statusEl.textContent = text
  }
}

new Mic()
