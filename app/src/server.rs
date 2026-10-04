//! Endpoint WHIP-like (`POST /whip`, SDP offer -> SDP answer) em localhost.
//! O Rails (HTTPS) repassa o offer do celular para cá.

use crate::{dsp, output};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use axum::{Router, extract::State, http::StatusCode, routing::post};
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
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

/// Estado compartilhado com a UI.
pub struct Shared {
    pub params: dsp::Params,
    pub connected: AtomicBool,
}

struct AppState {
    shared: Arc<Shared>,
    /// Apenas um celular por vez: a conexão anterior é derrubada.
    current: Mutex<Option<Arc<dyn PeerConnection>>>,
}

pub async fn serve(shared: Arc<Shared>, port: u16) -> Result<()> {
    let state = Arc::new(AppState { shared, current: Mutex::new(None) });
    let app = Router::new().route("/whip", post(whip)).with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn whip(State(st): State<Arc<AppState>>, offer_sdp: String) -> (StatusCode, String) {
    match negotiate(&st, offer_sdp).await {
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

async fn negotiate(st: &AppState, offer_sdp: String) -> Result<String> {
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

    let old = st.current.lock().unwrap().replace(pc);
    if let Some(old) = old {
        let _ = old.close().await;
    }
    Ok(sdp)
}
