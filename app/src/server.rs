//! Servidor HTTPS do app (porta pública na LAN + loopback):
//! - páginas e arquivos do celular (ver `web.rs`)
//! - `POST /offer`   SDP offer -> SDP answer (WebRTC)
//! - `DELETE /offer` encerra a conexão do celular
//!
//! O celular se identifica pelo cookie de pareamento.

use crate::jitter::{JitterBuffer, Pop};
use crate::state::{Link, Shared};
use crate::{dsp, output, recording, tls, web};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use axum_server::tls_rustls::RustlsConfig;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::net::{IpAddr, SocketAddr};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering::Relaxed},
};
use std::time::{Duration, Instant};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel, default_runtime};

struct Current {
    device: String,
    pc: Arc<dyn PeerConnection>,
}

pub struct AppState {
    pub shared: Arc<Shared>,
    /// Apenas um celular por vez.
    current: Mutex<Option<Current>>,
}

/// Sobe o servidor HTTPS na interface da LAN (`ip`) e no loopback.
pub async fn serve(shared: Arc<Shared>, ip: Option<IpAddr>, port: u16) -> Result<()> {
    // Provider de cripto único para o processo (o rustls não escolhe sozinho).
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cert = tls::ensure(ip)?;
    let config = RustlsConfig::from_pem(cert.cert_pem.into_bytes(), cert.key_pem.into_bytes())
        .await
        .map_err(|e| anyhow!("TLS: {e}"))?;

    let state = Arc::new(AppState {
        shared,
        current: Mutex::new(None),
    });
    let app = Router::new()
        .route("/offer", post(offer).delete(stop))
        .merge(web::routes())
        .with_state(state);

    // Escuta só na LAN e no loopback, não em todas as interfaces.
    let mut addrs = vec![SocketAddr::from(([127, 0, 0, 1], port))];
    if let Some(ip) = ip.filter(|i| !i.is_loopback()) {
        addrs.push(SocketAddr::new(ip, port));
    }
    let mut servers = tokio::task::JoinSet::new();
    for addr in addrs {
        let (cfg, app) = (config.clone(), app.clone());
        tracing::info!("servidor HTTPS em https://{addr}");
        servers.spawn(async move {
            axum_server::bind_rustls(addr, cfg)
                .serve(app.into_make_service())
                .await
        });
    }
    // Se qualquer listener cair, devolve o erro para o main registrar.
    while let Some(res) = servers.join_next().await {
        res??;
    }
    Ok(())
}

type Reply = (StatusCode, String);

/// Credenciais do cookie, se o celular estiver pareado.
fn paired(st: &AppState, h: &HeaderMap) -> Option<String> {
    let (id, secret) = web::credentials(h)?;
    st.shared.verify_device(&id, &secret).then_some(id)
}

async fn offer(State(st): State<Arc<AppState>>, h: HeaderMap, offer_sdp: String) -> Reply {
    let Some(device) = paired(&st, &h) else {
        return (StatusCode::FORBIDDEN, "celular não pareado".into());
    };
    // Outro celular só entra se o atual não estiver mais conectado/reconectando.
    let other = st
        .current
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|c| c.device != device);
    if other && st.shared.busy() {
        return (
            StatusCode::CONFLICT,
            "Já existe outro celular conectado".into(),
        );
    }
    match negotiate(&st, device, offer_sdp).await {
        Ok(answer) => (StatusCode::CREATED, answer),
        Err(e) => {
            tracing::error!("negociação falhou: {e:#}");
            st.shared.set_link(Link::Disconnected);
            (StatusCode::BAD_REQUEST, e.to_string())
        }
    }
}

/// O celular pediu para parar: derruba a conexão e mostra "desconectado" na hora.
async fn stop(State(st): State<Arc<AppState>>, h: HeaderMap) -> Reply {
    let Some(device) = paired(&st, &h) else {
        return (StatusCode::FORBIDDEN, "celular não pareado".into());
    };
    let cur = {
        let mut g = st.current.lock().unwrap();
        if g.as_ref().is_some_and(|c| c.device == device) {
            g.take()
        } else {
            None
        }
    };
    if let Some(c) = cur {
        st.shared.next_generation(); // ignora o "closed" que virá
        st.shared.set_link(Link::Disconnected);
        let _ = c.pc.close().await;
    }
    (StatusCode::NO_CONTENT, String::new())
}

struct Handler {
    runtime: Arc<dyn Runtime>,
    shared: Arc<Shared>,
    generation: u64,
    was_connected: AtomicBool,
    gather_done: Sender<()>,
}

#[async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_done.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        tracing::info!("conexão: {state}");
        let link = match state {
            RTCPeerConnectionState::Connected => {
                self.was_connected.store(true, Relaxed);
                Link::Connected
            }
            // Queda depois de ter conectado: o celular deve reconectar sozinho.
            RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed => {
                if self.was_connected.load(Relaxed) {
                    Link::Reconnecting
                } else {
                    Link::Disconnected
                }
            }
            RTCPeerConnectionState::Closed => return,
            _ => {
                if self.was_connected.load(Relaxed) {
                    Link::Reconnecting
                } else {
                    Link::Connecting
                }
            }
        };
        self.shared.set_link_for(self.generation, link);
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let (shared, generation) = (self.shared.clone(), self.generation);
        let runtime = self.runtime.clone();
        self.runtime.spawn(Box::pin(async move {
            if let Err(e) = pump(runtime, track, shared.clone(), generation).await {
                tracing::error!("pipeline de áudio: {e:#}");
                if shared.is_current(generation) {
                    shared.set_error(Some(format!("Falha no processamento de áudio: {e}")));
                }
            }
        }));
    }
}

/// RTP/Opus -> jitter buffer -> PCM -> DSP -> PipeWire.
///
/// Um receptor só enfileira os pacotes; a reprodução segue um relógio de 10 ms (um frame
/// do DSP), então a saída não depende do ritmo irregular da rede.
async fn pump(
    runtime: Arc<dyn Runtime>,
    track: Arc<dyn TrackRemote>,
    shared: Arc<Shared>,
    generation: u64,
) -> Result<()> {
    let _capture_session = recording::CaptureSession::new(shared.recorder.clone(), generation);
    let _monitor_session =
        crate::local_audio::MonitorSession::new(shared.local_audio.clone(), generation);
    let mut decoder = opus::Decoder::new(48_000, opus::Channels::Mono)?;
    let mut dsp = dsp::Processor::for_generation(&shared.params, generation);
    let mut sink = output::ResilientSink::new();
    let mut last_len = 960; // tamanho do último pacote, usado para reconstruir os perdidos
    let mut pcm = vec![0f32; 5760]; // até 120 ms
    let mut frames = FrameQueue::default();

    let jitter = Arc::new(Mutex::new(JitterBuffer::default()));
    let ended = Arc::new(AtomicBool::new(false));
    let receiver = {
        let (jitter, ended, shared) = (jitter.clone(), ended.clone(), shared.clone());
        runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                if !shared.is_current(generation) {
                    break; // este celular reconectou com outra conexão
                }
                if let TrackRemoteEvent::OnRtpPacket(pkt) = evt {
                    jitter.lock().unwrap().insert(
                        pkt.header.sequence_number,
                        pkt.header.timestamp,
                        pkt.payload.to_vec(),
                        Instant::now(),
                    );
                }
            }
            ended.store(true, Relaxed);
        }))
    };

    let mut clock = runtime.interval(Duration::from_millis(10));
    let result: Result<()> = async {
        loop {
            clock.tick().await;
            if !shared.is_current(generation) {
                return Ok(());
            }
            while frames.len() < dsp::FRAME {
                let popped = jitter.lock().unwrap().pop(Instant::now());
                let (input, fec) = match popped {
                    Pop::Wait => break,
                    Pop::Packet(p) if !p.is_empty() => {
                        let n = decoder.decode_float(&p, &mut pcm, false)?;
                        last_len = n;
                        frames.push(&pcm[..n]);
                        continue;
                    }
                    Pop::Packet(_) => (Vec::new(), false),
                    // Perda: o pacote seguinte pode trazer o FEC do que faltou.
                    Pop::Lost(next) => {
                        let fec = next.is_some();
                        (next.unwrap_or_default(), fec)
                    }
                };
                let out = &mut pcm[..last_len];
                let n = match decoder.decode_float(&input, out, fec) {
                    Ok(n) => n,
                    Err(_) => decoder.decode_float(&[], out, false)?,
                };
                frames.push(&pcm[..n]);
            }
            emit(&mut dsp, &mut sink, &mut frames, &shared, generation).await?;
            if ended.load(Relaxed) && jitter.lock().unwrap().len() == 0 {
                return Ok(());
            }
        }
    }
    .await;
    receiver.abort();
    result
}

/// Acumula o PCM decodificado e libera só frames inteiros do DSP. Pacotes Opus de
/// 2,5 ou 5 ms não fecham um frame de 10 ms; a sobra espera o próximo pacote em vez
/// de seguir sem tratamento.
#[derive(Default)]
struct FrameQueue {
    buf: Vec<f32>,
}

impl FrameQueue {
    fn push(&mut self, samples: &[f32]) {
        self.buf.extend_from_slice(samples);
    }
    fn len(&self) -> usize {
        self.buf.len()
    }
    /// Move até `max` frames completos para `out` e mantém o resto.
    fn take_frames(&mut self, out: &mut Vec<f32>, max: usize) {
        let whole = (self.buf.len() / dsp::FRAME).min(max) * dsp::FRAME;
        out.clear();
        out.extend(self.buf.drain(..whole));
    }
}

async fn emit(
    dsp: &mut dsp::Processor,
    sink: &mut output::ResilientSink,
    frames: &mut FrameQueue,
    shared: &Shared,
    generation: u64,
) -> Result<()> {
    let mut ready = Vec::new();
    frames.take_frames(&mut ready, 1);
    if ready.is_empty() {
        return Ok(());
    }
    prepare_output(
        dsp,
        &mut ready,
        &shared.params,
        &shared.recorder,
        &shared.monitor,
        generation,
    );
    sink.write_mono(&ready).await;
    Ok(())
}

/// Mantém as rotas locais antes do controle de saída do microfone virtual.
pub(crate) fn prepare_output(
    dsp: &mut dsp::Processor,
    pcm: &mut [f32],
    params: &dsp::Params,
    recorder: &recording::Recorder,
    monitor: &crate::monitor::Monitor,
    generation: u64,
) {
    recording::process_block(dsp, pcm, params, recorder, generation);
    crate::monitor::submit_processed(monitor, generation, pcm);
    if params.muted.load(std::sync::atomic::Ordering::Relaxed) {
        pcm.fill(0.0);
    }
}

async fn negotiate(st: &AppState, device: String, offer_sdp: String) -> Result<String> {
    let generation = st.shared.next_generation();
    st.shared.set_error(None);
    st.shared.set_link(Link::Connecting);
    let mut media = MediaEngine::default();
    media.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RtpCodecKind::Audio,
    )?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    let runtime = default_runtime().ok_or_else(|| anyhow!("sem runtime"))?;
    let (gather_tx, mut gather_rx) = channel::<()>(1);

    let handler = Arc::new(Handler {
        runtime: runtime.clone(),
        shared: st.shared.clone(),
        generation,
        was_connected: AtomicBool::new(false),
        gather_done: gather_tx,
    });

    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build()) // sem STUN: só LAN
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_handler(handler as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{}:0", crate::local_ip())])
        .build()
        .await?;
    let pc: Arc<dyn PeerConnection> = Arc::new(pc);

    pc.add_transceiver_from_kind(
        RtpCodecKind::Audio,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            ..Default::default()
        }),
    )
    .await?;

    pc.set_remote_description(RTCSessionDescription::offer(offer_sdp)?)
        .await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;
    futures::select! {
        _ = gather_rx.recv().fuse() => {}
        _ = tokio::time::sleep(std::time::Duration::from_secs(3)).fuse() => {}
    }
    let sdp = pc
        .local_description()
        .await
        .ok_or_else(|| anyhow!("sem answer"))?
        .sdp;

    let old = st.current.lock().unwrap().replace(Current { device, pc });
    if let Some(old) = old {
        let _ = old.pc.close().await;
    }
    Ok(sdp)
}

#[cfg(test)]
mod pump_tests {
    use super::*;

    #[test]
    fn frame_queue_releases_only_whole_frames_and_keeps_the_remainder() {
        let mut q = FrameQueue::default();
        let mut out = Vec::new();
        // pacotes de 5 ms (240): só o segundo fecha 480
        q.push(&vec![0.1; 240]);
        q.take_frames(&mut out, usize::MAX);
        assert!(out.is_empty());
        q.push(&vec![0.2; 240]);
        q.take_frames(&mut out, usize::MAX);
        assert_eq!(out.len(), dsp::FRAME);
        // pacote de 20 ms (960): dois frames, sem sobra
        q.push(&vec![0.3; 960]);
        q.take_frames(&mut out, usize::MAX);
        assert_eq!(out.len(), 2 * dsp::FRAME);
        // 120 + 960 = 1080: dois frames e sobra de 120
        q.push(&vec![0.4; 120]);
        q.push(&vec![0.5; 960]);
        q.take_frames(&mut out, usize::MAX);
        assert_eq!(out.len(), 2 * dsp::FRAME);
        q.take_frames(&mut out, usize::MAX);
        assert!(out.is_empty());
        assert_eq!(q.buf.len(), 120);
        assert_eq!(q.buf[0], 0.5, "a sobra preserva a ordem");
    }

    #[test]
    fn opus_fec_and_plc_recover_burst_loss_without_failing() {
        let mut enc =
            opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        enc.set_inband_fec(true).unwrap();
        enc.set_packet_loss_perc(20).unwrap();
        let mut dec = opus::Decoder::new(48_000, opus::Channels::Mono).unwrap();
        let tone: Vec<f32> = (0..960).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let mut pkts = Vec::new();
        for _ in 0..4 {
            pkts.push(enc.encode_vec_float(&tone, 400).unwrap());
        }
        let mut out = vec![0f32; 960];
        dec.decode_float(&pkts[0], &mut out, false).unwrap();
        // pacotes 1 e 2 perdidos: PLC para o 1, FEC (do pacote 3) para o 2
        assert_eq!(dec.decode_float(&[], &mut out, false).unwrap(), 960);
        assert_eq!(dec.decode_float(&pkts[3], &mut out, true).unwrap(), 960);
        assert_eq!(dec.decode_float(&pkts[3], &mut out, false).unwrap(), 960);
    }
}
