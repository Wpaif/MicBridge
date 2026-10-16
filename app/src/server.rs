//! Servidor HTTPS do app (porta pública na LAN + loopback):
//! - páginas e arquivos do celular (ver `web.rs`)
//! - `POST /offer`   SDP offer -> SDP answer (WebRTC)
//! - `DELETE /offer` encerra a conexão do celular
//!
//! O celular se identifica pelo cookie de pareamento.

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
        self.runtime.spawn(Box::pin(async move {
            if let Err(e) = pump(track, shared.clone(), generation).await {
                tracing::error!("pipeline de áudio: {e:#}");
                if shared.is_current(generation) {
                    shared.set_error(Some(format!("Falha no processamento de áudio: {e}")));
                }
            }
        }));
    }
}

/// RTP/Opus -> PCM -> DSP -> PipeWire.
async fn pump(track: Arc<dyn TrackRemote>, shared: Arc<Shared>, generation: u64) -> Result<()> {
    let _capture_session = recording::CaptureSession::new(shared.recorder.clone(), generation);
    let mut decoder = opus::Decoder::new(48_000, opus::Channels::Mono)?;
    let mut dsp = dsp::Processor::new();
    let mut sink = output::ResilientSink::new();
    let mut last_seq: Option<u16> = None;
    let mut pcm = vec![0f32; 5760]; // até 120 ms

    while let Some(evt) = track.poll().await {
        if !shared.is_current(generation) {
            break; // este celular reconectou com outra conexão
        }
        let TrackRemoteEvent::OnRtpPacket(pkt) = evt else {
            continue;
        };
        let seq = pkt.header.sequence_number;

        // Perda de exatamente 1 pacote: conceal (PLC) do Opus.
        if let Some(prev) = last_seq {
            let gap = seq.wrapping_sub(prev);
            if gap == 0 || gap > 0x8000 {
                continue; // duplicado / atrasado
            }
            if gap == 2 {
                let n = decoder.decode_float(&[], &mut pcm[..960], false)?;
                emit(&mut dsp, &mut sink, &mut pcm[..n], &shared, generation).await?;
            }
        }
        last_seq = Some(seq);

        if pkt.payload.is_empty() {
            continue;
        }
        let n = decoder.decode_float(&pkt.payload, &mut pcm, false)?;
        emit(&mut dsp, &mut sink, &mut pcm[..n], &shared, generation).await?;
    }
    Ok(())
}

async fn emit(
    dsp: &mut dsp::Processor,
    sink: &mut output::ResilientSink,
    pcm: &mut [f32],
    shared: &Shared,
    generation: u64,
) -> Result<()> {
    recording::process_block(dsp, pcm, &shared.params, &shared.recorder, generation);
    sink.write_mono(pcm).await;
    Ok(())
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
