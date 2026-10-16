//! Estado compartilhado entre servidor WebRTC, supervisor do PipeWire, UI e bandeja.

use crate::dsp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering::Relaxed},
};

/// Duração da janela para parear um novo celular.
pub const PAIRING_SECS: u64 = 300;
/// Quanto tempo "reconectando" dura antes de virar "desconectado".
const RECONNECT_GRACE_SECS: u64 = 30;

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `n` bytes aleatórios em hex.
pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("sem entropia do sistema");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Dados persistentes do app (`~/.local/share/micbridge`): pareamento e certificados.
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("micbridge")
}

/// Comparação em tempo constante.
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn sha256_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

// ───────────────────────── Celulares pareados (persistentes) ─────────────────────────

#[derive(Serialize, Deserialize)]
struct Device {
    secret_sha256: String,
    created: u64,
}

/// Guarda só o hash do segredo de cada celular, em `~/.local/share/micbridge/devices.json` (0600).
struct Store {
    path: PathBuf,
    devices: HashMap<String, Device>,
}

impl Store {
    fn load() -> Self {
        let path = data_dir().join("devices.json");
        let devices = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, devices }
    }

    fn save(&self) {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let json = serde_json::to_string_pretty(&self.devices)?;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&self.path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, json.as_bytes()))
        };
        if let Err(e) = write() {
            tracing::error!("não consegui salvar os celulares pareados: {e}");
        }
    }
}

// ───────────────────────── Estado de conexão ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Link {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Reconnecting = 3,
    Error = 4,
}

impl Link {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Reconnecting,
            4 => Self::Error,
            _ => Self::Disconnected,
        }
    }
}

pub struct Status {
    pub link: Link,
    pub label: &'static str,
    pub detail: String,
}

pub struct Shared {
    pub params: dsp::Params,
    pub recorder: crate::recording::Recorder,
    /// Token do QR code (válido só com a janela de pareamento aberta).
    pub pair_token: String,
    link: AtomicU8,
    link_since: AtomicU64,
    generation: AtomicU64,
    pairing_until: AtomicU64,
    pw_ok: AtomicBool,
    last_error: Mutex<Option<String>>,
    store: Mutex<Store>,
}

impl Shared {
    pub fn new() -> Self {
        let store = Store::load();
        let first_run = store.devices.is_empty();
        let s = Self {
            params: Default::default(),
            recorder: crate::recording::Recorder::new(),
            pair_token: random_hex(16),
            link: AtomicU8::new(Link::Disconnected as u8),
            link_since: AtomicU64::new(now_secs()),
            generation: AtomicU64::new(0),
            pairing_until: AtomicU64::new(0),
            pw_ok: AtomicBool::new(true),
            last_error: Mutex::new(None),
            store: Mutex::new(store),
        };
        // QR só na primeira utilização; depois o celular é reconhecido.
        if first_run {
            s.open_pairing();
        }
        s
    }

    // ── pareamento ──
    pub fn open_pairing(&self) {
        self.pairing_until.store(now_secs() + PAIRING_SECS, Relaxed);
    }
    pub fn close_pairing(&self) {
        self.pairing_until.store(0, Relaxed);
    }
    pub fn pairing_remaining(&self) -> u64 {
        self.pairing_until.load(Relaxed).saturating_sub(now_secs())
    }
    pub fn device_count(&self) -> usize {
        self.store.lock().unwrap().devices.len()
    }

    /// Registra um celular novo e devolve `(id, segredo)`. Fecha a janela de pareamento.
    pub fn add_device(&self) -> (String, String) {
        let (id, secret) = (random_hex(8), random_hex(32));
        let mut st = self.store.lock().unwrap();
        st.devices.insert(
            id.clone(),
            Device {
                secret_sha256: sha256_hex(&secret),
                created: now_secs(),
            },
        );
        st.save();
        self.close_pairing();
        (id, secret)
    }

    pub fn verify_device(&self, id: &str, secret: &str) -> bool {
        let st = self.store.lock().unwrap();
        st.devices
            .get(id)
            .is_some_and(|d| ct_eq(&d.secret_sha256, &sha256_hex(secret)))
    }

    pub fn forget_all(&self) {
        let mut st = self.store.lock().unwrap();
        st.devices.clear();
        st.save();
    }

    // ── conexão ──
    /// Nova geração a cada negociação: eventos de conexões antigas são ignorados.
    pub fn next_generation(&self) -> u64 {
        self.recorder.disconnect(self.generation.load(Relaxed));
        self.generation.fetch_add(1, Relaxed) + 1
    }
    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Relaxed) == generation
    }
    pub fn set_link(&self, link: Link) {
        if matches!(link, Link::Disconnected | Link::Reconnecting | Link::Error) {
            self.recorder.disconnect(self.generation.load(Relaxed));
        }
        if self.link.swap(link as u8, Relaxed) != link as u8 {
            self.link_since.store(now_secs(), Relaxed);
        }
    }
    /// Só aplica se `generation` ainda for a conexão atual.
    pub fn set_link_for(&self, generation: u64, link: Link) {
        if self.is_current(generation) {
            self.set_link(link);
        }
    }
    pub fn set_error(&self, msg: Option<String>) {
        *self.last_error.lock().unwrap() = msg;
    }
    pub fn set_pw_ok(&self, ok: bool) {
        self.pw_ok.store(ok, Relaxed);
    }

    fn link(&self) -> Link {
        let l = Link::from_u8(self.link.load(Relaxed));
        if l == Link::Reconnecting
            && now_secs().saturating_sub(self.link_since.load(Relaxed)) > RECONNECT_GRACE_SECS
        {
            return Link::Disconnected;
        }
        l
    }

    /// O celular está (ou deve voltar a estar) usando o app: bloqueia outros aparelhos.
    pub fn busy(&self) -> bool {
        matches!(self.link(), Link::Connected | Link::Reconnecting)
    }

    /// Estado único para UI e bandeja: conectado, desconectado, reconectando ou erro.
    pub fn status(&self) -> Status {
        if !self.pw_ok.load(Relaxed) {
            return Status {
                link: Link::Error,
                label: "Erro",
                detail: "Microfone virtual indisponível: recriando…".into(),
            };
        }
        if let Some(e) = self.last_error.lock().unwrap().clone() {
            return Status {
                link: Link::Error,
                label: "Erro",
                detail: e,
            };
        }
        match self.link() {
            Link::Connected => Status {
                link: Link::Connected,
                label: "Conectado",
                detail: "Recebendo áudio do celular".into(),
            },
            Link::Connecting => Status {
                link: Link::Connecting,
                label: "Conectando",
                detail: "Negociando a conexão…".into(),
            },
            Link::Reconnecting => Status {
                link: Link::Reconnecting,
                label: "Reconectando",
                detail: "Conexão perdida. Aguardando o celular voltar…".into(),
            },
            _ => Status {
                link: Link::Disconnected,
                label: "Desconectado",
                detail: if self.device_count() == 0 {
                    "Escaneie o QR code com o celular".into()
                } else {
                    "Abra a página do MicBridge no celular".into()
                },
            },
        }
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn connection_loss_stops_capture_without_waiting_for_packets() {
        for link in [Link::Disconnected, Link::Reconnecting, Link::Error] {
            let s = Shared::new();
            let generation = s.next_generation();
            s.set_link(Link::Connected);
            s.recorder.start(generation);
            s.set_link_for(generation, link);
            assert!(s.recorder.active_id(generation).is_none(), "{link:?}");
        }
    }
    #[test]
    fn reconnect_stops_old_capture_and_stale_callbacks_do_not_stop_new_one() {
        let s = Shared::new();
        let old = s.next_generation();
        s.recorder.start(old);
        let new = s.next_generation();
        assert!(s.recorder.active_id(old).is_none());
        let id = s.recorder.start(new);
        s.set_link_for(old, Link::Disconnected);
        assert_eq!(s.recorder.active_id(new), Some(id));
    }
}
