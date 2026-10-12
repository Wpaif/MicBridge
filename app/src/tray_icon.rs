//! Ícone da bandeja desenhado em tempo de execução (sem arquivos de imagem).
//!
//! Microfone branco (com um contorno escuro fino, para não sumir em painéis
//! claros) e uma bolinha de status no canto livre, sem tocar no microfone. Cores
//! do Discord: verde = conectado, amarelo = conectando, vermelho = erro, cinza =
//! desconectado. A bolinha pisca nos estados transitórios, e ganha um anel que
//! pulsa quando há voz chegando.

use crate::state::Link;
use ksni::Icon;

/// Tamanhos entregues ao host da bandeja, que escolhe o mais próximo.
const SIZES: [u32; 4] = [22, 32, 48, 64];
/// Amostras por eixo em cada pixel (suavização das bordas).
const SS: u32 = 4;

type Rgb = [f32; 3];

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

// Cores de status do Discord.
const GREEN: Rgb = rgb(0x23, 0xA5, 0x5A);
const YELLOW: Rgb = rgb(0xF0, 0xB2, 0x32);
const RED: Rgb = rgb(0xF2, 0x3F, 0x43);
const GRAY: Rgb = rgb(0x80, 0x84, 0x8E);
const WHITE: Rgb = [1.0, 1.0, 1.0];
const OUTLINE: Rgb = [0.0, 0.0, 0.0];
/// Espessura e opacidade do contorno escuro do microfone.
const OUTLINE_W: f32 = 0.035;
const OUTLINE_A: f32 = 0.5;

// Geometria em coordenadas unitárias (0..1, y para baixo).
const MIC_X: f32 = 0.36;
/// Bolinha no canto inferior direito, fora da área do microfone.
const DOT: (f32, f32, f32) = (0.80, 0.80, 0.12);
const HALO: (f32, f32) = (0.16, 0.21);

/// Aparência do ícone num instante: estado, se há voz, e a fase (0/1) da animação.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Look {
    pub link: Link,
    pub speaking: bool,
    pub phase: u8,
}

struct Badge {
    color: Rgb,
    alpha: f32,
    halo: f32,
}

impl Look {
    /// Estados transitórios e de erro pedem animação; os demais ficam parados.
    pub fn animated(link: Link, speaking: bool) -> bool {
        match link {
            Link::Connected => speaking,
            Link::Connecting | Link::Reconnecting | Link::Error => true,
            Link::Disconnected => false,
        }
    }

    fn badge(self) -> Badge {
        let on = self.phase % 2 == 0;
        let blink = |lo: f32| if on { 1.0 } else { lo };
        match self.link {
            Link::Connected => Badge {
                color: GREEN,
                alpha: 1.0,
                halo: match (self.speaking, on) {
                    (false, _) => 0.0,
                    (true, true) => 0.65,
                    (true, false) => 0.25,
                },
            },
            Link::Connecting | Link::Reconnecting => Badge {
                color: YELLOW,
                alpha: blink(0.3),
                halo: 0.0,
            },
            Link::Error => Badge {
                color: RED,
                alpha: blink(0.25),
                halo: 0.0,
            },
            Link::Disconnected => Badge {
                color: GRAY,
                alpha: 1.0,
                halo: 0.0,
            },
        }
    }
}

/// Todos os tamanhos do ícone para a aparência dada.
pub fn icons(look: Look) -> Vec<Icon> {
    SIZES.iter().map(|&s| render(s, look)).collect()
}

fn dist(u: f32, v: f32, cx: f32, cy: f32) -> f32 {
    ((u - cx).powi(2) + (v - cy).powi(2)).sqrt()
}

/// Distância do ponto ao segmento (a, b).
fn seg(u: f32, v: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((u - a.0) * abx + (v - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    dist(u, v, a.0 + t * abx, a.1 + t * aby)
}

/// Distância com sinal ao microfone: negativa dentro, positiva fora (aproximada).
fn mic_dist(u: f32, v: f32) -> f32 {
    let x = MIC_X;
    let capsule = seg(u, v, (x, 0.17), (x, 0.43)) - 0.13;
    // Suporte em "U": metade inferior de um anel.
    let holder = if v >= 0.43 {
        (dist(u, v, x, 0.43) - 0.20).abs() - 0.03
    } else {
        f32::MAX
    };
    let stem = seg(u, v, (x, 0.64), (x, 0.84)) - 0.03;
    let base = seg(u, v, (x - 0.13, 0.86), (x + 0.13, 0.86)) - 0.03;
    capsule.min(holder).min(stem).min(base)
}

/// Cor (alfa pré-multiplicado) de um ponto da imagem.
fn sample(u: f32, v: f32, badge: &Badge) -> [f32; 4] {
    let over = |dst: [f32; 4], c: Rgb, a: f32| {
        let k = 1.0 - a;
        [
            c[0] * a + dst[0] * k,
            c[1] * a + dst[1] * k,
            c[2] * a + dst[2] * k,
            a + dst[3] * k,
        ]
    };

    let mut px = [0.0; 4];
    let d_mic = mic_dist(u, v);
    if d_mic <= OUTLINE_W {
        px = over(px, OUTLINE, OUTLINE_A);
    }
    if d_mic <= 0.0 {
        px = over(px, WHITE, 1.0);
    }
    let d_dot = dist(u, v, DOT.0, DOT.1);
    if badge.halo > 0.0 && d_dot >= HALO.0 && d_dot <= HALO.1 {
        px = over(px, badge.color, badge.halo);
    }
    if d_dot <= DOT.2 {
        px = over(px, badge.color, badge.alpha);
    }
    px
}

fn render(size: u32, look: Look) -> Icon {
    let badge = look.badge();
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let n = (SS * SS) as f32;

    for y in 0..size {
        for x in 0..size {
            let mut acc = [0.0_f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let s = sample(u, v, &badge);
                    for (a, c) in acc.iter_mut().zip(s) {
                        *a += c;
                    }
                }
            }
            let a = acc[3] / n;
            // StatusNotifierItem espera ARGB32 sem pré-multiplicação.
            let straight = |c: f32| {
                if a > 0.0 {
                    ((c / n / a).clamp(0.0, 1.0) * 255.0).round() as u8
                } else {
                    0
                }
            };
            data.extend_from_slice(&[
                (a * 255.0).round() as u8,
                straight(acc[0]),
                straight(acc[1]),
                straight(acc[2]),
            ]);
        }
    }

    Icon {
        width: size as i32,
        height: size as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(icon: &Icon, fx: f32, fy: f32) -> [u8; 4] {
        let x = (fx * icon.width as f32) as usize;
        let y = (fy * icon.height as f32) as usize;
        let i = (y * icon.width as usize + x) * 4;
        icon.data[i..i + 4].try_into().unwrap()
    }

    fn look(link: Link, speaking: bool, phase: u8) -> Look {
        Look {
            link,
            speaking,
            phase,
        }
    }

    #[test]
    fn tamanhos_e_buffers_consistentes() {
        for icon in icons(look(Link::Connected, false, 0)) {
            assert_eq!(icon.data.len(), (icon.width * icon.height * 4) as usize);
        }
    }

    #[test]
    fn microfone_branco_sobre_fundo_transparente() {
        let icon = render(64, look(Link::Disconnected, false, 0));
        assert_eq!(px(&icon, 0.9, 0.1)[0], 0); // canto livre
        assert_eq!(px(&icon, MIC_X, 0.3), [255, 255, 255, 255]); // corpo do microfone
    }

    #[test]
    fn bolinha_nao_toca_no_microfone() {
        let mut y = 0.0;
        while y <= 1.0 {
            let mut x = 0.0;
            while x <= 1.0 {
                if dist(x, y, DOT.0, DOT.1) <= HALO.1 {
                    assert!(
                        mic_dist(x, y) > OUTLINE_W,
                        "({x:.2},{y:.2}) invade o microfone"
                    );
                }
                x += 0.01;
            }
            y += 0.01;
        }
    }

    #[test]
    fn bolinha_usa_a_cor_do_estado() {
        for (link, color) in [
            (Link::Connected, GREEN),
            (Link::Connecting, YELLOW),
            (Link::Error, RED),
            (Link::Disconnected, GRAY),
        ] {
            let icon = render(64, look(link, false, 0));
            let [a, r, g, b] = px(&icon, DOT.0, DOT.1);
            let want = color.map(|c| (c * 255.0).round() as u8);
            assert_eq!(a, 255, "{link:?}");
            assert_eq!([r, g, b], want, "{link:?}");
        }
    }

    #[test]
    fn bolinha_pisca_entre_as_fases() {
        let alpha = |phase| {
            px(
                &render(64, look(Link::Reconnecting, false, phase)),
                DOT.0,
                DOT.1,
            )[0]
        };
        assert!(alpha(0) > alpha(1));
    }

    #[test]
    fn anel_so_aparece_com_voz() {
        // Ponto no anel, sobre o canto inferior direito.
        let r = (HALO.0 + HALO.1) / 2.0;
        let (u, v) = (DOT.0 + r * 0.7, DOT.1 + r * 0.7);
        let quiet = px(&render(64, look(Link::Connected, false, 0)), u, v);
        let talking = px(&render(64, look(Link::Connected, true, 0)), u, v);
        assert_eq!(quiet[0], 0);
        assert!(talking[0] > 0);
    }

    #[test]
    fn so_anima_quando_precisa() {
        assert!(!Look::animated(Link::Disconnected, true));
        assert!(!Look::animated(Link::Connected, false));
        assert!(Look::animated(Link::Connected, true));
        assert!(Look::animated(Link::Error, false));
        assert!(Look::animated(Link::Reconnecting, false));
    }
}
