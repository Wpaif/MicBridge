pub struct GainRamp {
    current: f32,
    from: f32,
    target: f32,
    remaining: usize,
}
impl GainRamp {
    pub fn new(percent: f32) -> Self {
        let gain = if percent.is_finite() {
            percent.clamp(0.0, 100.0) / 100.0
        } else {
            0.25
        };
        Self {
            current: gain,
            from: gain,
            target: gain,
            remaining: 0,
        }
    }
    pub fn set_percent(&mut self, percent: f32) {
        if !percent.is_finite() {
            return;
        }
        let target = percent.clamp(0.0, 100.0) / 100.0;
        if target == self.target {
            return;
        }
        self.from = self.current;
        self.target = target;
        self.remaining = 240;
    }
    pub fn apply(&mut self, samples: &mut [f32]) {
        for sample in samples {
            if self.remaining > 0 {
                self.remaining -= 1;
                self.current =
                    self.from + (self.target - self.from) * (240 - self.remaining) as f32 / 240.0;
            }
            *sample = if sample.is_finite() {
                *sample * self.current
            } else {
                0.0
            };
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_ramps_over_240_samples() {
        let mut g = GainRamp::new(25.0);
        g.set_percent(100.0);
        let mut x = [1.0; 241];
        g.apply(&mut x);
        assert!((x[0] - 0.253125).abs() < 1e-6);
        assert!((x[239] - 1.0).abs() < 1e-6);
        assert_eq!(x[240], 1.0);
    }
    #[test]
    fn stable_volume_invalid_values_and_sanitized_samples() {
        for (percent, expected) in [
            (0.0, 0.0),
            (25.0, 0.25),
            (100.0, 1.0),
            (-10.0, 0.0),
            (110.0, 1.0),
        ] {
            let mut g = GainRamp::new(percent);
            let mut x = [1.0, f32::NAN, f32::INFINITY];
            g.apply(&mut x);
            assert_eq!(x, [expected, 0.0, 0.0]);
            g.set_percent(f32::NAN);
            g.set_percent(f32::INFINITY);
            let mut x = [1.0; 240];
            g.apply(&mut x);
            assert_eq!(x[239], expected);
        }
    }
    #[test]
    fn new_target_starts_from_current_gain_without_jump() {
        let mut g = GainRamp::new(0.0);
        g.set_percent(100.0);
        let mut half = [1.0; 120];
        g.apply(&mut half);
        assert!((half[119] - 0.5).abs() < 1e-6);
        g.set_percent(0.0);
        let mut tail = [1.0; 240];
        g.apply(&mut tail);
        assert!((tail[0] - (0.5 - 0.5 / 240.0)).abs() < 1e-6);
        assert_eq!(tail[239], 0.0);
    }
}
