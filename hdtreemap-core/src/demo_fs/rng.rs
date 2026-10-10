//! A small, self-contained, seeded PRNG for the demo file-tree generator (`demo_fs`).
//!
//! SplitMix64: one `u64` of state, fast, and plenty random for picking file names and sizes.
//! Being our own code (not a crate) keeps the generated trees identical across platforms and
//! dependency upgrades: same seed, same tree, forever, which the determinism tests rely on.

/// SplitMix64 generator. See the module docs.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        // Lemire's multiply-shift; the tiny bias is irrelevant here.
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + self.below(hi - lo + 1)
    }

    /// Uniform `usize` in `lo..=hi`.
    pub fn range_usize(&mut self, lo: usize, hi: usize) -> usize {
        self.range(lo as u64, hi as u64) as usize
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal sample (Box–Muller).
    pub fn gaussian(&mut self) -> f64 {
        let u1 = self.unit().max(1e-12);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// A uniformly chosen element (`items` must not be empty; the first one otherwise).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let i = self.below(items.len() as u64) as usize;
        &items[i.min(items.len().saturating_sub(1))]
    }

    /// An element chosen with probability proportional to its weight.
    pub fn weighted<'a, T>(&mut self, items: &'a [(T, u32)]) -> &'a T {
        let total: u64 = items.iter().map(|(_, w)| u64::from(*w)).sum();
        let mut roll = self.below(total.max(1));
        for (item, weight) in items {
            let w = u64::from(*weight);
            if roll < w {
                return item;
            }
            roll -= w;
        }
        &items[items.len().saturating_sub(1)].0
    }

    /// `n` lower-case hex digits.
    pub fn hex(&mut self, n: usize) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(n);
        let mut bits = 0u64;
        for i in 0..n {
            if i % 16 == 0 {
                bits = self.next_u64();
            }
            s.push(DIGITS[(bits & 0xF) as usize] as char);
            bits >>= 4;
        }
        s
    }

    /// A child generator with its own stream, derived from this one.
    pub fn fork(&mut self) -> Rng {
        Rng::new(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            assert!(r.below(10) < 10);
            let x = r.range(5, 8);
            assert!((5..=8).contains(&x));
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
        }
        assert_eq!(r.below(0), 0);
        assert_eq!(r.range(9, 3), 9);
    }

    #[test]
    fn weighted_picks_follow_the_weights() {
        let mut r = Rng::new(3);
        let items = [("never", 0), ("rare", 1), ("common", 99)];
        let mut common = 0;
        for _ in 0..1000 {
            let pick = *r.weighted(&items);
            assert_ne!(pick, "never");
            if pick == "common" {
                common += 1;
            }
        }
        assert!(common > 950);
    }

    #[test]
    fn hex_has_the_requested_length() {
        let mut r = Rng::new(9);
        let h = r.hex(40);
        assert_eq!(h.len(), 40);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn gaussian_is_roughly_standard() {
        let mut r = Rng::new(11);
        let n = 20_000;
        let samples: Vec<f64> = (0..n).map(|_| r.gaussian()).collect();
        let mean = samples.iter().sum::<f64>() / n as f64;
        let var = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.05, "mean {mean}");
        assert!((var - 1.0).abs() < 0.08, "variance {var}");
    }
}
