//! Endpoint WHIP-like (`POST /whip`, SDP offer -> SDP answer) em localhost.
//! O Rails (HTTPS) repassa o offer do celular para cá.

use crate::{dsp, output};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel, default_runtime};

/// Estado compartilhado com a UI.
pub struct Shared {
    pub params: dsp::Params,
    pub connected: AtomicBool,
    /// Segredo de pareamento, embutido no QR code e repassado ao Rails.
    pub token: String,
    /// Fim da janela de pareamento (segundos Unix); 0 = fechada.
    pairing_until: AtomicU64,
    /// Dispositivos autorizados nesta sessão.
    paired: Mutex<HashSet<String>>,
}

/// Duração da janela de pareamento.
pub const PAIRING_SECS: u64 = 300;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Shared {
    /// Nasce com a janela de pareamento aberta.
    pub fn new(token: String) -> Self {
        let s = Self {
            params: Default::default(),
            connected: Default::default(),
            token,
            pairing_until: AtomicU64::new(0),
            paired: Default::default(),
        };
        s.open_pairing();
        s
    }
    pub fn open_pairing(&self) {
        self.pairing_until.store(now_secs() + PAIRING_SECS, Relaxed);
    }
    pub fn close_pairing(&self) {
        self.pairing_until.store(0, Relaxed);
    }
    /// Segundos restantes da janela (0 = fechada).
    pub fn pairing_remaining(&self) -> u64 {
        self.pairing_until.load(Relaxed).saturating_sub(now_secs())
    }
}

/// Token aleatório de 128 bits (hex).
pub fn new_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("sem entropia do sistema");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Comparação em tempo constante.
fn token_ok(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given.bytes().zip(expected.bytes()).fold(0, |a, (x, y)| a | (x ^ y)) == 0
}

struct AppState {
    shared: Arc<Shared>,
    /// Apenas um celular por vez: (id do dispositivo, conexão).
    current: Mutex<Option<(String, Arc<dyn PeerConnection>)>>,
}

pub async fn serve(shared: Arc<Shared>, port: u16) -> Result<()> {
    let state = Arc::new(AppState { shared, current: Mutex::new(None) });
    let app = Router::new().route("/pair", post(pair))
        .route("/whip", post(whip)).with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Autoriza um dispositivo. Só aceita novos enquanto a janela de pareamento estiver aberta;
/// dispositivos já pareados sempre passam. A janela fecha após um novo pareamento.
async fn pair(State(st): State<Arc<AppState>>, headers: HeaderMap) -> (StatusCode, String) {
    let hdr = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !token_ok(hdr("x-micbridge-token"), &st.shared.token) {
        return (StatusCode::UNAUTHORIZED, "token inválido".into());
    }
    let device = hdr("x-micbridge-device");
    if device.is_empty() {
        return (StatusCode::BAD_REQUEST, "dispositivo ausente".into());
    }
    let mut paired = st.shared.paired.lock().unwrap();
    if paired.contains(device) {
        return (StatusCode::OK, "ok".into());
    }
    if st.shared.pairing_remaining() == 0 {
        return (StatusCode::FORBIDDEN, "pareamento fechado".into());
    }
    paired.insert(device.to_owned());
    st.shared.close_pairing();
    tracing::info!("novo dispositivo pareado");
    (StatusCode::CREATED, "ok".into())
}

async fn whip(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    offer_sdp: String,
) -> (StatusCode, String) {
    let hdr = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !token_ok(hdr("x-micbridge-token"), &st.shared.token) {
        return (StatusCode::UNAUTHORIZED, "token inválido".into());
    }
    let device = hdr("x-micbridge-device").to_owned();
    if device.is_empty() {
        return (StatusCode::BAD_REQUEST, "dispositivo ausente".into());
    }
    if !st.shared.paired.lock().unwrap().contains(&device) {
        return (StatusCode::FORBIDDEN, "dispositivo não pareado".into());
    }
    // Outro dispositivo só entra se o atual não estiver mais conectado.
    let busy = st
        .current
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|(d, _)| *d != device)
        && st.shared.connected.load(Relaxed);
    if busy {
        return (StatusCode::CONFLICT, "Já existe outro celular conectado".into());
    }
    match negotiate(&st, device, offer_sdp).await {
        Ok(answer) => (StatusCode::CREATED, answer),
        Err(e) => {
            tracing::error!("negociação falhou: {e:#}");
            (StatusCode::BAD_REQUEST, e.to_string())
        }
    }
}

struct Handler {
    runtime: Arc<dyn Runtime>,
    shared: Arc<Shared>,
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
        self.shared
            .connected
            .store(state == RTCPeerConnectionState::Connected, Relaxed);
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let shared = self.shared.clone();
        self.runtime.spawn(Box::pin(async move {
            if let Err(e) = pump(track, shared).await {
                tracing::error!("pipeline de áudio: {e:#}");
            }
        }));
    }
}

/// RTP/Opus -> PCM -> DSP -> PipeWire.
async fn pump(track: Arc<dyn TrackRemote>, shared: Arc<Shared>) -> Result<()> {
    let mut decoder = opus::Decoder::new(48_000, opus::Channels::Mono)?;
    let mut dsp = dsp::Processor::new();
    let mut sink = output::Sink::open()?;
    let mut last_seq: Option<u16> = None;
    let mut pcm = vec![0f32; 5760]; // até 120 ms

    while let Some(evt) = track.poll().await {
        let TrackRemoteEvent::OnRtpPacket(pkt) = evt else { continue };
        let seq = pkt.header.sequence_number;

        // Perda de exatamente 1 pacote: conceal (PLC) do Opus.
        if let Some(prev) = last_seq {
            let gap = seq.wrapping_sub(prev);
            if gap == 0 || gap > 0x8000 {
                continue; // duplicado / atrasado
            }
            if gap == 2 {
                let n = decoder.decode_float(&[], &mut pcm[..960], false)?;
                emit(&mut dsp, &mut sink, &mut pcm[..n], &shared).await?;
            }
        }
        last_seq = Some(seq);

        if pkt.payload.is_empty() {
            continue;
        }
        let n = decoder.decode_float(&pkt.payload, &mut pcm, false)?;
        emit(&mut dsp, &mut sink, &mut pcm[..n], &shared).await?;
    }
    Ok(())
}

async fn emit(
    dsp: &mut dsp::Processor,
    sink: &mut output::Sink,
    pcm: &mut [f32],
    shared: &Shared,
) -> Result<()> {
    for frame in pcm.chunks_exact_mut(dsp::FRAME) {
        dsp.process(frame, &shared.params);
    }
    sink.write_mono(pcm).await
}

async fn negotiate(st: &AppState, device: String, offer_sdp: String) -> Result<String> {
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

    pc.set_remote_description(RTCSessionDescription::offer(offer_sdp)?).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;
    futures::select! {
        _ = gather_rx.recv().fuse() => {}
        _ = tokio::time::sleep(std::time::Duration::from_secs(3)).fuse() => {}
    }
    let sdp = pc.local_description().await.ok_or_else(|| anyhow!("sem answer"))?.sdp;

    let old = st.current.lock().unwrap().replace((device, pc));
    if let Some((_, old)) = old {
        let _ = old.close().await;
    }
    Ok(sdp)
}
