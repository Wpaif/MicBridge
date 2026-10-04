//! Ícone na bandeja (StatusNotifierItem), como Discord/Steam. No GNOME requer
//! a extensão AppIndicator.

use crate::server::Shared;
use ksni::{Category, Icon, MenuItem, ToolTip, Tray, TrayMethods, menu::StandardItem};
use std::sync::{Arc, atomic::Ordering::Relaxed};
use std::time::Duration;

/// PNG -> pixmap ARGB32 (big-endian) exigido pelo StatusNotifierItem.
fn pixmap(png_bytes: &[u8]) -> Icon {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .expect("png embutido");
    let mut buf = vec![0; reader.output_buffer_size().expect("png embutido")];
    let info = reader.next_frame(&mut buf).expect("png embutido");
    let data = buf[..info.buffer_size()]
        .chunks_exact(4)
        .flat_map(|p| [p[3], p[0], p[1], p[2]]) // RGBA -> ARGB
        .collect();
    Icon { width: info.width as i32, height: info.height as i32, data }
}

fn icons(connected: bool) -> Vec<Icon> {
    if connected {
        vec![
            pixmap(include_bytes!("../data/tray-active-32.png")),
            pixmap(include_bytes!("../data/tray-active-64.png")),
        ]
    } else {
        vec![
            pixmap(include_bytes!("../data/tray-idle-32.png")),
            pixmap(include_bytes!("../data/tray-idle-64.png")),
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Show,
    Quit,
}

struct MicTray {
    tx: async_channel::Sender<TrayEvent>,
    connected: bool,
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
        icons(self.connected)
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "MicBridge".into(),
            description: if self.connected {
                "Celular conectado, transmitindo áudio".into()
            } else {
                "Aguardando celular".into()
            },
            icon_pixmap: icons(self.connected),
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
                label: if self.connected {
                    "● Celular conectado".into()
                } else {
                    "○ Aguardando celular".into()
                },
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

/// Deve ser chamado dentro do runtime tokio. Atualiza o ícone conforme a conexão.
pub async fn spawn(tx: async_channel::Sender<TrayEvent>, shared: Arc<Shared>) {
    let handle = match (MicTray { tx, connected: false }).spawn().await {
        Ok(h) => h,
        Err(e) => return tracing::warn!("bandeja indisponível: {e}"),
    };
    let mut last = false;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let now = shared.connected.load(Relaxed);
        if now != last {
            last = now;
            if handle.update(|t| t.connected = now).await.is_none() {
                break; // bandeja encerrada
            }
        }
    }
}
