//! Jitter buffer do áudio recebido: reordena pacotes RTP, absorve variação de chegada e
//! decide, a cada pedido do relógio de reprodução, entre tocar, reconstruir ou esperar.
//!
//! A profundidade-alvo acompanha o jitter medido (RFC 3550): sobe na hora quando a rede
//! piora e desce devagar quando ela estabiliza, para não oscilar o atraso.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Relógio RTP do Opus.
const CLOCK_KHZ: f32 = 48.0;
const DEFAULT_PACKET_MS: f32 = 20.0;
const MIN_TARGET: usize = 1;
const MAX_TARGET: usize = 6;
const INITIAL_TARGET: usize = 2;
/// Acima disso a fila está atrasada demais: descarta o excesso e volta ao alvo.
const MAX_DEPTH: usize = 10;
/// Sem pacotes à frente, reconstrói no máximo isto antes de voltar a encher o buffer.
const MAX_CONCEALED_RUN: usize = 5;
/// Espera máxima pelo enchimento inicial antes de começar com o que houver.
const MAX_FILL_WAIT: Duration = Duration::from_millis(200);
/// Estabilidade exigida antes de reduzir a profundidade em um pacote.
const SHRINK_AFTER: Duration = Duration::from_secs(5);
/// Salto de sequência que indica um novo fluxo, não perda.
const RESET_JUMP: i64 = 3000;
/// Evita índices negativos ao estender o número de sequência.
const SEQ_BASE: i64 = 1 << 32;

#[derive(Debug, PartialEq, Eq)]
pub enum Pop {
    /// Próximo pacote, na ordem. Payload vazio é silêncio (DTX) e vira reconstrução.
    Packet(Vec<u8>),
    /// Pacote perdido. Traz o pacote seguinte, de onde o Opus pode recuperar via FEC.
    Lost(Option<Vec<u8>>),
    /// Nada a tocar agora (enchendo o buffer).
    Wait,
}

#[derive(Debug)]
pub struct JitterBuffer {
    packets: BTreeMap<i64, Vec<u8>>,
    /// Próximo número de sequência (estendido) a tocar.
    next: Option<i64>,
    highest: Option<i64>,
    playing: bool,
    fill_since: Option<Instant>,
    concealed_run: usize,
    target: usize,
    last_target_change: Option<Instant>,
    packet_ms: f32,
    jitter_ms: f32,
    last_arrival: Option<(i64, u32, Instant)>,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self {
            packets: BTreeMap::new(),
            next: None,
            highest: None,
            playing: false,
            fill_since: None,
            concealed_run: 0,
            target: INITIAL_TARGET,
            last_target_change: None,
            packet_ms: DEFAULT_PACKET_MS,
            jitter_ms: 0.0,
            last_arrival: None,
        }
    }
}

impl JitterBuffer {
    #[cfg(test)]
    pub fn target_packets(&self) -> usize {
        self.target
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    fn extend(&self, seq: u16) -> i64 {
        match self.highest {
            None => SEQ_BASE + i64::from(seq),
            Some(h) => h + i64::from(seq.wrapping_sub(h as u16) as i16),
        }
    }

    fn reset(&mut self) {
        let target = self.target;
        *self = Self::default();
        self.target = target;
    }

    pub fn insert(&mut self, seq: u16, timestamp: u32, payload: Vec<u8>, now: Instant) {
        let ext = self.extend(seq);
        if self.highest.is_some_and(|h| (ext - h).abs() > RESET_JUMP) {
            self.reset();
        }
        let ext = self.extend(seq);
        if self.playing && self.next.is_some_and(|n| ext < n) {
            return; // chegou depois da hora de tocar
        }
        self.measure(ext, timestamp, now);
        self.highest = Some(self.highest.map_or(ext, |h| h.max(ext)));
        self.packets.insert(ext, payload);
    }

    /// Atualiza duração do pacote, jitter e profundidade-alvo.
    fn measure(&mut self, ext: i64, timestamp: u32, now: Instant) {
        if let Some((prev_ext, prev_ts, prev_at)) = self.last_arrival
            && ext == prev_ext + 1
        {
            let ts_ms = timestamp.wrapping_sub(prev_ts) as f32 / CLOCK_KHZ;
            if (2.5..=120.0).contains(&ts_ms) {
                self.packet_ms = ts_ms;
                let arrival_ms = now.saturating_duration_since(prev_at).as_secs_f32() * 1000.0;
                let d = (arrival_ms - ts_ms).abs();
                self.jitter_ms += (d - self.jitter_ms) / 16.0;
                self.retarget(now);
            }
        }
        if self.last_arrival.is_none_or(|(e, _, _)| ext > e) {
            self.last_arrival = Some((ext, timestamp, now));
        }
    }

    fn retarget(&mut self, now: Instant) {
        let want_ms = (3.0 * self.jitter_ms).max(self.packet_ms);
        let want = ((want_ms / self.packet_ms).ceil() as usize).clamp(MIN_TARGET, MAX_TARGET);
        if want > self.target {
            self.target = want;
            self.last_target_change = Some(now);
        } else if want < self.target {
            let stable = self
                .last_target_change
                .is_none_or(|t| now.saturating_duration_since(t) >= SHRINK_AFTER);
            if stable {
                self.target -= 1;
                self.last_target_change = Some(now);
            }
        }
    }

    pub fn pop(&mut self, now: Instant) -> Pop {
        if !self.playing && !self.try_start(now) {
            return Pop::Wait;
        }
        self.trim_excess();
        let Some(next) = self.next else {
            return Pop::Wait;
        };
        if let Some(payload) = self.packets.remove(&next) {
            self.next = Some(next + 1);
            self.concealed_run = 0;
            return Pop::Packet(payload);
        }
        // Falta o pacote `next`.
        if self.highest.is_some_and(|h| h > next) {
            let fec = self.packets.get(&(next + 1)).cloned();
            self.next = Some(next + 1);
            return Pop::Lost(fec);
        }
        // Nada à frente: pode ser só atraso. Reconstrói um pouco e, se persistir, reenche.
        self.concealed_run += 1;
        if self.concealed_run > MAX_CONCEALED_RUN {
            self.playing = false;
            self.fill_since = None;
            self.concealed_run = 0;
            self.packets.clear();
            self.next = None;
            self.highest = None;
            return Pop::Wait;
        }
        self.next = Some(next + 1);
        Pop::Lost(None)
    }

    fn try_start(&mut self, now: Instant) -> bool {
        let since = *self.fill_since.get_or_insert(now);
        let waited = now.saturating_duration_since(since);
        let ready = self.packets.len() >= self.target
            || (!self.packets.is_empty() && waited >= MAX_FILL_WAIT);
        if ready {
            self.next = self.packets.keys().next().copied();
            self.playing = true;
        }
        ready
    }

    /// Fila longa demais (rajada atrasada): pula para manter só o alvo, sem reconstruir.
    fn trim_excess(&mut self) {
        let (Some(next), Some(high)) = (self.next, self.highest) else {
            return;
        };
        if (high - next + 1) as usize > MAX_DEPTH {
            let keep_from = high - self.target as i64 + 1;
            self.packets = self.packets.split_off(&keep_from);
            self.next = Some(keep_from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn ts(i: u32) -> u32 {
        i * 960
    }
    fn pkt(i: u8) -> Vec<u8> {
        vec![i]
    }

    #[test]
    fn waits_until_target_then_plays_in_order_after_reordering() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(11, ts(1), pkt(1), t0);
        assert_eq!(jb.pop(t0), Pop::Wait, "abaixo do alvo");
        jb.insert(10, ts(0), pkt(0), t0 + 1 * MS);
        assert_eq!(jb.pop(t0 + 2 * MS), Pop::Packet(pkt(0)));
        assert_eq!(jb.pop(t0 + 3 * MS), Pop::Packet(pkt(1)));
    }

    #[test]
    fn starts_with_what_it_has_after_max_fill_wait() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(1, ts(0), pkt(0), t0);
        assert_eq!(jb.pop(t0), Pop::Wait);
        assert_eq!(jb.pop(t0 + MAX_FILL_WAIT), Pop::Packet(pkt(0)));
    }

    #[test]
    fn reports_loss_with_fec_source_and_drops_late_packet() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(1, ts(0), pkt(0), t0);
        jb.insert(3, ts(2), pkt(2), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(0)));
        assert_eq!(jb.pop(t0), Pop::Lost(Some(pkt(2))));
        jb.insert(2, ts(1), pkt(1), t0 + 5 * MS); // tarde demais
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(2)));
    }

    #[test]
    fn burst_loss_yields_one_lost_per_missing_packet() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(1, ts(0), pkt(0), t0);
        jb.insert(4, ts(3), pkt(3), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(0)));
        assert_eq!(jb.pop(t0), Pop::Lost(None));
        assert_eq!(jb.pop(t0), Pop::Lost(Some(pkt(3))));
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(3)));
    }

    #[test]
    fn underrun_conceals_then_refills() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(1, ts(0), pkt(0), t0);
        jb.insert(2, ts(1), pkt(1), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(0)));
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(1)));
        for _ in 0..MAX_CONCEALED_RUN {
            assert_eq!(jb.pop(t0), Pop::Lost(None));
        }
        assert_eq!(jb.pop(t0), Pop::Wait, "voltou a encher");
        jb.insert(30, ts(29), pkt(9), t0);
        jb.insert(31, ts(30), pkt(10), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(9)));
    }

    #[test]
    fn sequence_wraparound_stays_in_order() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(65535, ts(0), pkt(0), t0);
        jb.insert(0, ts(1), pkt(1), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(0)));
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(1)));
    }

    #[test]
    fn jittery_arrivals_raise_target_and_calm_lowers_it_slowly() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        let mut at = t0;
        // chegadas alternando 5 ms e 35 ms (média 20 ms): jitter ~15 ms
        for i in 0..60u32 {
            at += if i % 2 == 0 { 5 * MS } else { 35 * MS };
            jb.insert((i + 1) as u16, ts(i), pkt(0), at);
        }
        assert!(jb.target_packets() >= 3, "alvo {}", jb.target_packets());
        let raised = jb.target_packets();
        // rede estável por 20 s
        let base = 60u32;
        for i in 0..1000u32 {
            at += 20 * MS;
            jb.insert((base + i + 1) as u16, ts(base + i), pkt(0), at);
        }
        assert!(jb.target_packets() < raised, "alvo {}", jb.target_packets());
        assert!(jb.target_packets() >= MIN_TARGET);
    }

    #[test]
    fn long_queue_is_trimmed_back_to_target() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        for i in 0..30u32 {
            jb.insert((i + 1) as u16, ts(i), pkt(i as u8), t0);
        }
        let Pop::Packet(first) = jb.pop(t0) else {
            panic!("esperava pacote");
        };
        assert!(
            first[0] >= 30 - jb.target_packets() as u8,
            "tocou {first:?}"
        );
        assert!(jb.len() <= jb.target_packets());
    }

    #[test]
    fn large_sequence_jump_restarts_the_stream() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::default();
        jb.insert(1, ts(0), pkt(0), t0);
        jb.insert(2, ts(1), pkt(1), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(0)));
        jb.insert(40000, ts(0), pkt(7), t0);
        jb.insert(40001, ts(1), pkt(8), t0);
        assert_eq!(jb.pop(t0), Pop::Packet(pkt(7)));
    }
}
