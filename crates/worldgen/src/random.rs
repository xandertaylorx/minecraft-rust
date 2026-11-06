//! Xoroshiro128++ random source and positional factories, bit-exact with Minecraft.

pub const GOLDEN_RATIO_64: i64 = -7046029254386353131;
pub const SILVER_RATIO_64: i64 = 7640891576956012809;

pub fn mix_stafford13(mut z: i64) -> i64 {
    z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(-4658895280553007687);
    z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(-7723592293110705685);
    z ^ ((z as u64) >> 31) as i64
}

#[derive(Clone, Copy, Debug)]
pub struct Seed128 {
    pub lo: i64,
    pub hi: i64,
}

impl Seed128 {
    pub fn upgrade(seed: i64) -> Seed128 {
        let lo = seed ^ SILVER_RATIO_64;
        let hi = lo.wrapping_add(GOLDEN_RATIO_64);
        Seed128 { lo: mix_stafford13(lo), hi: mix_stafford13(hi) }
    }
    pub fn from_hash_of(s: &str) -> Seed128 {
        let d = md5::compute(s.as_bytes()).0;
        let lo = i64::from_be_bytes(d[0..8].try_into().unwrap());
        let hi = i64::from_be_bytes(d[8..16].try_into().unwrap());
        Seed128 { lo, hi }
    }
    pub fn xor(self, lo: i64, hi: i64) -> Seed128 {
        Seed128 { lo: self.lo ^ lo, hi: self.hi ^ hi }
    }
}

#[derive(Clone, Debug)]
pub struct Xoro {
    lo: i64,
    hi: i64,
}

impl Xoro {
    pub fn new(seed: i64) -> Self { Self::from_seed128(Seed128::upgrade(seed)) }
    pub fn from_seed128(s: Seed128) -> Self { Self::from_parts(s.lo, s.hi) }
    pub fn from_parts(lo: i64, hi: i64) -> Self {
        if lo | hi == 0 { Self { lo: GOLDEN_RATIO_64, hi: SILVER_RATIO_64 } } else { Self { lo, hi } }
    }

    pub fn next_long(&mut self) -> i64 {
        let (i, mut j) = (self.lo, self.hi);
        let k = i.wrapping_add(j).rotate_left(17).wrapping_add(i);
        j ^= i;
        self.lo = i.rotate_left(49) ^ j ^ (j << 21);
        self.hi = j.rotate_left(28);
        k
    }

    pub fn next_int(&mut self) -> i32 { self.next_long() as i32 }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0);
        let b = bound as u32 as u64;
        let mut i = self.next_int() as u32 as u64;
        let mut j = i * b;
        let mut k = j & 0xFFFF_FFFF;
        if k < b {
            let l = (!(bound as u32)).wrapping_add(1) % (bound as u32);
            while k < l as u64 {
                i = self.next_int() as u32 as u64;
                j = i * b;
                k = j & 0xFFFF_FFFF;
            }
        }
        (j >> 32) as i32
    }

    pub fn next_boolean(&mut self) -> bool { self.next_long() & 1 != 0 }

    fn next_bits(&mut self, bits: u32) -> i64 { ((self.next_long() as u64) >> (64 - bits)) as i64 }

    pub fn next_float(&mut self) -> f32 { self.next_bits(24) as f32 * 5.9604645E-8f32 }

    /// Java evaluates `long * float` in single precision, then widens.
    pub fn next_double(&mut self) -> f64 { (self.next_bits(53) as f32 * 1.110223E-16f32) as f64 }

    pub fn consume_count(&mut self, n: i32) { for _ in 0..n { self.next_long(); } }

    pub fn fork(&mut self) -> Xoro {
        let (a, b) = (self.next_long(), self.next_long());
        Xoro::from_parts(a, b)
    }

    pub fn fork_positional(&mut self) -> Positional {
        let (a, b) = (self.next_long(), self.next_long());
        Positional { lo: a, hi: b }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Positional {
    lo: i64,
    hi: i64,
}

impl Positional {
    pub fn at(&self, x: i32, y: i32, z: i32) -> Xoro {
        let seed = get_seed(x, y, z);
        Xoro::from_parts(seed ^ self.lo, self.hi)
    }
    pub fn from_hash_of(&self, name: &str) -> Xoro {
        Xoro::from_seed128(Seed128::from_hash_of(name).xor(self.lo, self.hi))
    }
    pub fn from_seed(&self, seed: i64) -> Xoro { Xoro::from_parts(seed ^ self.lo, seed ^ self.hi) }
}

/// `Mth.getSeed(x, y, z)`.
pub fn get_seed(x: i32, y: i32, z: i32) -> i64 {
    let i = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    let i = i.wrapping_mul(i).wrapping_mul(42317861).wrapping_add(i.wrapping_mul(11));
    i >> 16
}
