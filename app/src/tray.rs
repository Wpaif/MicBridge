//! Ícone na bandeja (StatusNotifierItem), como Discord/Steam. No GNOME requer
//! a extensão AppIndicator.

use crate::state::{Link, Shared};
use crate::tray_icon::{self, Look};
use ksni::{Category, Icon, MenuItem, ToolTip, Tray, TrayMethods, menu::StandardItem};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Nível RMS de saída (linear) a partir do qual consideramos que há voz (~-46 dBFS).
const SPEAKING_RMS: f32 = 0.005;
/// A bolinha segue "falando" por um instante, para não piscar entre as palavras.
const SPEAKING_HOLD: Duration = Duration::from_millis(600);
/// Metade do ciclo de piscar.
const BLINK_MS: u128 = 500;

#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Show,
    Quit,
}

struct MicTray {
    tx: async_channel::Sender<TrayEvent>,
    look: Look,
    label: &'static str,
    detail: String,
}

impl Tray for MicTray {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }
    fn title(&self) -> String {
        "MicBridge".into()
    }
    fn category(&self) -> Category {
        Category::ApplicationStatus
    }
    // Ícone próprio embutido no binário: não depende do tema de ícones instalado.
    fn icon_pixmap(&self) -> Vec<Icon> {
        tray_icon::icons(self.look)
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "MicBridge".into(),
            description: format!("{}: {}", self.label, self.detail),
            icon_pixmap: tray_icon::icons(self.look),
            ..Default::default()
        }
    }
    // Clique esquerdo: abre a janela.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(TrayEvent::Show);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: format!(
                    "{} {}",
                    match self.look.link {
                        Link::Connected => "🟢",
                        Link::Connecting | Link::Reconnecting => "🟡",
                        Link::Error => "🔴",
                        Link::Disconnected => "⚪",
                    },
                    self.label
                ),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Abrir MicBridge".into(),
                icon_name: "audio-input-microphone-symbolic".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.try_send(TrayEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Sair".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.try_send(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Deve ser chamado dentro do runtime tokio. Reflete o estado da conexão no ícone,
/// pisca nos estados transitórios e pulsa quando há voz chegando.
pub async fn spawn(tx: async_channel::Sender<TrayEvent>, shared: Arc<Shared>) {
    let st = shared.status();
    let idle = Look {
        link: st.link,
        speaking: false,
        phase: 0,
    };
    let tray = MicTray {
        tx,
        look: idle,
        label: st.label,
        detail: st.detail,
    };
    let handle = match tray.spawn().await {
        Ok(h) => h,
        Err(e) => return tracing::warn!("bandeja indisponível: {e}"),
    };

    let started = Instant::now();
    let mut speak_until = started;
    let mut last_frames = shared.params.frames();
    let mut last = (idle, st.label, String::new());
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let now = Instant::now();
        let st = shared.status();

        // Voz: o contador de frames garante que o nível medido não é um valor antigo.
        let frames = shared.params.frames();
        if frames != last_frames && shared.params.out_level() > SPEAKING_RMS {
            speak_until = now + SPEAKING_HOLD;
        }
        last_frames = frames;
        let speaking = st.link == Link::Connected && now < speak_until;

        let phase = if Look::animated(st.link, speaking) {
            ((now - started).as_millis() / BLINK_MS % 2) as u8
        } else {
            0
        };
        let look = Look {
            link: st.link,
            speaking,
            phase,
        };

        let current = (look, st.label, st.detail.clone());
        if current != last {
            last = current;
            let updated = handle
                .update(|t| {
                    t.look = look;
                    t.label = st.label;
                    t.detail = st.detail;
                })
                .await;
            if updated.is_none() {
                break; // bandeja encerrada
            }
        }
    }
}
