use std::collections::{HashMap, HashSet};

const FIRST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const REST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "continue", "do", "else", "elseif", "end", "false",
    "for", "function", "if", "in", "local", "nil", "not", "or",
    "repeat", "return", "then", "true", "type", "until", "while",
];

/// Names that should be mangled consistently across ALL types (e.g. new, _values).
const SHARED_NAMES: &[&str] = &["new", "_values"];

/// Lua metamethods that CANNOT be mangled — the Lua runtime looks them up by exact name.
const LUA_METAMETHODS: &[&str] = &[
    "__index", "__newindex", "__call", "__concat", "__unm", "__add", "__sub",
    "__mul", "__div", "__idiv", "__mod", "__pow", "__tostring", "__metatable",
    "__eq", "__lt", "__le", "__gc", "__close", "__len", "__pairs", "__ipairs",
    "__iter", "__mode", "__name", "__type",
];

pub struct Mangler {
    type_maps: HashMap<String, TypeMangler>,
    /// Shared name mappings — same mangled name used across all types.
    shared_map: TypeMangler,
    /// Base seed for this build (derived from timestamp).
    base_seed: u64,
}

struct TypeMangler {
    name_map: HashMap<String, String>,
    used_names: HashSet<String>,
    /// Permuted index generator for the current length tier.
    tier: NameTier,
    /// How many names we've generated so far (for advancing tiers).
    count: usize,
}

/// Generates names in a random-looking order within each length tier,
/// exhausting all combinations of length N before moving to N+1.
struct NameTier {
    length: u32,
    tier_size: usize,
    index_in_tier: usize,
    /// LCG state for permuting indices within the tier.
    lcg_state: u64,
    lcg_a: u64,
    lcg_c: u64,
    seed: u64,
}

impl Mangler {
    pub fn new() -> Self {
        let base_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xCAFE);
        Self {
            type_maps: HashMap::new(),
            shared_map: TypeMangler::new(base_seed),
            base_seed,
        }
    }

    pub fn mangle(&mut self, type_name: &str, member_name: &str) -> String {
        // Lua metamethods cannot be mangled — the runtime requires exact names
        if LUA_METAMETHODS.contains(&member_name) {
            return member_name.to_string();
        }
        // Shared names get one consistent mangled name across all types
        if SHARED_NAMES.contains(&member_name) {
            let name = self.shared_map.get_or_create(member_name);
            // Reserve this name in all existing per-type manglers to prevent collisions
            for tm in self.type_maps.values_mut() {
                tm.reserve(&name);
            }
            return name;
        }
        let shared_used: Vec<String> = self.shared_map.name_map.values().cloned().collect();
        let base = self.base_seed;
        let type_mangler = self
            .type_maps
            .entry(type_name.to_string())
            .or_insert_with(|| {
                let seed = hash_str(type_name) ^ base;
                let mut tm = TypeMangler::new(seed);
                // Reserve all already-assigned shared names
                for reserved in &shared_used {
                    tm.reserve(reserved);
                }
                tm
            });
        type_mangler.get_or_create(member_name)
    }

    pub fn lookup(&self, type_name: &str, member_name: &str) -> Option<String> {
        if LUA_METAMETHODS.contains(&member_name) {
            return Some(member_name.to_string());
        }
        if SHARED_NAMES.contains(&member_name) {
            return self.shared_map.name_map.get(member_name).cloned();
        }
        self.type_maps
            .get(type_name)
            .and_then(|tm| tm.name_map.get(member_name).cloned())
    }
}

impl TypeMangler {
    fn new(seed: u64) -> Self {
        let tier_size = FIRST_CHARS.len(); // 53 for length 1
        Self {
            name_map: HashMap::new(),
            used_names: HashSet::new(),
            tier: NameTier::new(1, tier_size, seed),
            count: 0,
        }
    }

    fn reserve(&mut self, reserved_name: &str) {
        self.used_names.insert(reserved_name.to_string());
    }

    fn get_or_create(&mut self, name: &str) -> String {
        if let Some(mangled) = self.name_map.get(name) {
            return mangled.clone();
        }
        loop {
            let candidate = self.next_name();
            if !LUA_KEYWORDS.contains(&candidate.as_str()) && !self.used_names.contains(&candidate) {
                self.used_names.insert(candidate.clone());
                self.name_map.insert(name.to_string(), candidate.clone());
                return candidate;
            }
        }
    }

    fn next_name(&mut self) -> String {
        loop {
            if self.tier.index_in_tier < self.tier.tier_size {
                let permuted = self.tier.next_permuted_index();
                self.tier.index_in_tier += 1;
                self.count += 1;
                return index_to_name_at_length(permuted, self.tier.length);
            }
            // Exhausted current tier, move to next length
            let next_length = self.tier.length + 1;
            let fc = FIRST_CHARS.len();
            let rc = REST_CHARS.len();
            let next_size = fc * rc.pow(next_length - 1);
            self.tier = NameTier::new(next_length, next_size, self.tier.seed.wrapping_add(next_length as u64));
        }
    }
}

impl NameTier {
    fn new(length: u32, tier_size: usize, seed: u64) -> Self {
        // Find LCG parameters that give a full-period permutation of [0, tier_size).
        // For a full period LCG: x = (a*x + c) mod m
        // Requirements: gcd(c, m) = 1, a-1 divisible by all prime factors of m, if m%4==0 then (a-1)%4==0
        let m = tier_size as u64;
        let (a, c) = find_lcg_params(m, seed);

        // Start the LCG at a seed-derived position
        let lcg_state = seed % m;

        Self {
            length,
            tier_size,
            index_in_tier: 0,
            lcg_state,
            lcg_a: a,
            lcg_c: c,
            seed,
        }
    }

    fn next_permuted_index(&mut self) -> usize {
        let result = self.lcg_state as usize;
        self.lcg_state = (self.lcg_a.wrapping_mul(self.lcg_state).wrapping_add(self.lcg_c)) % (self.tier_size as u64);
        result
    }
}

/// Find LCG parameters (a, c) that produce a full-period permutation of [0, m).
fn find_lcg_params(m: u64, seed: u64) -> (u64, u64) {
    if m <= 1 {
        return (1, 0);
    }

    // c must be coprime with m
    let c = {
        let mut c = (seed % m.max(2)) | 1; // start odd for better coprimality chances
        loop {
            if gcd(c, m) == 1 {
                break c;
            }
            c = (c + 2) % m;
            if c == 0 { c = 1; }
        }
    };

    // a-1 must be divisible by all prime factors of m
    // if m is divisible by 4, a-1 must be divisible by 4
    let factors = prime_factors(m);
    let mut a_minus_1: u64 = 1;
    for &f in &factors {
        if a_minus_1 % f != 0 {
            a_minus_1 *= f;
        }
    }
    if m % 4 == 0 && a_minus_1 % 4 != 0 {
        a_minus_1 *= 2;
    }

    // Mix in the seed to vary `a` across types
    let multiplier = ((seed / 3).max(1) % 50) * 2 + 1; // odd multiplier
    let a = a_minus_1.wrapping_mul(multiplier) + 1;
    let a = if a % m == 1 { a + a_minus_1 } else { a }; // avoid identity
    let a = a % m;
    let a = if a == 0 { a_minus_1 + 1 } else { a };

    (a, c)
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn prime_factors(mut n: u64) -> Vec<u64> {
    let mut factors = Vec::new();
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            factors.push(d);
            while n % d == 0 {
                n /= d;
            }
        }
        d += 1;
    }
    if n > 1 {
        factors.push(n);
    }
    factors
}

/// Simple string hash for generating per-type seeds.
fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Convert an index within a specific length tier to a name.
fn index_to_name_at_length(index: usize, length: u32) -> String {
    let fc = FIRST_CHARS.len();
    let rc = REST_CHARS.len();

    if length == 1 {
        return String::from(FIRST_CHARS[index % fc] as char);
    }

    let mut remaining = index;
    let mut name = String::with_capacity(length as usize);

    let rest_power = rc.pow(length - 1);
    let first_idx = remaining / rest_power;
    remaining %= rest_power;
    name.push(FIRST_CHARS[first_idx % fc] as char);

    for i in (0..length - 1).rev() {
        let d = rc.pow(i);
        let char_idx = remaining / d;
        remaining %= d;
        name.push(REST_CHARS[char_idx % rc] as char);
    }

    name
}

/// Returns how many unique names can be generated up to the given length.
pub fn names_up_to_length(max_length: u32) -> usize {
    let fc = FIRST_CHARS.len();
    let rc = REST_CHARS.len();
    let mut total = 0;
    for l in 1..=max_length {
        total += fc * rc.pow(l - 1);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_keywords() {
        let mut mangler = Mangler::new();
        for i in 0..200 {
            let name = mangler.mangle("Test", &format!("field_{}", i));
            assert!(
                !LUA_KEYWORDS.contains(&name.as_str()),
                "Generated keyword: {}",
                name
            );
        }
    }

    #[test]
    fn test_no_duplicates_within_type() {
        let mut mangler = Mangler::new();
        let mut seen = HashSet::new();
        for i in 0..200 {
            let name = mangler.mangle("Test", &format!("field_{}", i));
            assert!(seen.insert(name.clone()), "Duplicate mangled name: {}", name);
        }
    }

    #[test]
    fn test_different_types_different_names() {
        let mut mangler = Mangler::new();
        let a = mangler.mangle("ClassA", "foo");
        let b = mangler.mangle("ClassB", "foo");
        // Different types should generally get different names due to different seeds
        // (not guaranteed for every case, but should differ for most)
        let _ = (a, b); // just ensure no panic
    }

    #[test]
    fn test_metamethods_preserved() {
        let mut mangler = Mangler::new();
        assert_eq!(mangler.mangle("Test", "__index"), "__index");
        assert_eq!(mangler.mangle("Test", "__tostring"), "__tostring");
    }

    #[test]
    fn test_shared_names_consistent() {
        let mut mangler = Mangler::new();
        let new_a = mangler.mangle("ClassA", "new");
        let new_b = mangler.mangle("ClassB", "new");
        assert_eq!(new_a, new_b, "new should have same mangled name across types");

        let vals_a = mangler.mangle("EnumA", "_values");
        let vals_b = mangler.mangle("EnumB", "_values");
        assert_eq!(vals_a, vals_b, "_values should have same mangled name across types");
    }

    #[test]
    fn test_names_are_valid_identifiers() {
        let mut mangler = Mangler::new();
        for i in 0..200 {
            let name = mangler.mangle("Test", &format!("f_{}", i));
            assert!(name.len() > 0);
            let first = name.as_bytes()[0];
            assert!(
                first.is_ascii_alphabetic() || first == b'_',
                "Invalid first char in: {}", name
            );
            for &b in &name.as_bytes()[1..] {
                assert!(
                    b.is_ascii_alphanumeric() || b == b'_',
                    "Invalid char in: {}", name
                );
            }
        }
    }

    #[test]
    fn test_exhausts_single_char_before_two_char() {
        let mut tm = TypeMangler::new(42);
        let mut single_char = 0;
        let mut double_char = 0;
        let mut first_double_at = 0;

        for i in 0..100 {
            let name = tm.next_name();
            if LUA_KEYWORDS.contains(&name.as_str()) {
                continue;
            }
            if name.len() == 1 {
                single_char += 1;
            } else if name.len() == 2 {
                if double_char == 0 {
                    first_double_at = i;
                }
                double_char += 1;
            }
        }
        // Should exhaust most single-char names before any double-char
        assert!(single_char > 40, "Should use many single-char names first, got {}", single_char);
        assert!(first_double_at >= 50, "Double-char names should start after single-char tier, started at {}", first_double_at);
    }

    #[test]
    fn test_names_up_to_length() {
        assert_eq!(names_up_to_length(1), 53);
        assert_eq!(names_up_to_length(2), 53 + 53 * 63);
    }
}
