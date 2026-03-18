use std::collections::HashMap;

const FIRST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const REST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "continue", "do", "else", "elseif", "end", "false",
    "for", "function", "if", "in", "local", "nil", "not", "or",
    "repeat", "return", "then", "true", "type", "until", "while",
];

/// Names that should be mangled consistently across ALL types (e.g. _new, _values).
const SHARED_NAMES: &[&str] = &["_new", "_values"];

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
}

struct TypeMangler {
    name_map: HashMap<String, String>,
    next_index: usize,
}

impl Mangler {
    pub fn new() -> Self {
        Self {
            type_maps: HashMap::new(),
            shared_map: TypeMangler::new(),
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
        let type_mangler = self
            .type_maps
            .entry(type_name.to_string())
            .or_insert_with(|| {
                let mut tm = TypeMangler::new();
                // Reserve all already-assigned shared names
                for reserved in self.shared_map.name_map.values() {
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
    fn new() -> Self {
        Self {
            name_map: HashMap::new(),
            next_index: 0,
        }
    }

    /// Reserve a generated name so it won't be assigned to any future member.
    fn reserve(&mut self, reserved_name: &str) {
        // Skip indices that would generate this name
        loop {
            let candidate = index_to_name(self.next_index);
            if candidate == reserved_name {
                self.next_index += 1;
            } else {
                break;
            }
        }
    }

    fn get_or_create(&mut self, name: &str) -> String {
        if let Some(mangled) = self.name_map.get(name) {
            return mangled.clone();
        }
        loop {
            let candidate = index_to_name(self.next_index);
            self.next_index += 1;
            if !LUA_KEYWORDS.contains(&candidate.as_str()) {
                self.name_map.insert(name.to_string(), candidate.clone());
                return candidate;
            }
        }
    }
}

/// Converts a 0-based index to a valid Lua identifier.
///
/// Length 1: 53 names (a-z, A-Z, _)
/// Length L: 53 * 63^(L-1) names
///
/// Uses direct computation (no enumeration) for efficiency.
fn index_to_name(index: usize) -> String {
    let fc = FIRST_CHARS.len(); // 53
    let rc = REST_CHARS.len(); // 63

    if index < fc {
        return String::from(FIRST_CHARS[index] as char);
    }

    let mut remaining = index - fc;
    let mut length: u32 = 2;
    let mut count = fc * rc;

    while remaining >= count {
        remaining -= count;
        length += 1;
        count *= rc;
    }

    let mut name = String::with_capacity(length as usize);
    let rest_power = rc.pow(length - 1);
    let first_idx = remaining / rest_power;
    remaining %= rest_power;
    name.push(FIRST_CHARS[first_idx] as char);

    for i in (0..length - 1).rev() {
        let d = rc.pow(i);
        let char_idx = remaining / d;
        remaining %= d;
        name.push(REST_CHARS[char_idx] as char);
    }

    name
}

/// Returns how many unique names can be generated up to the given length.
/// Useful for capacity estimation.
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
    fn test_single_char_names() {
        assert_eq!(index_to_name(0), "a");
        assert_eq!(index_to_name(25), "z");
        assert_eq!(index_to_name(26), "A");
        assert_eq!(index_to_name(51), "Z");
        assert_eq!(index_to_name(52), "_");
    }

    #[test]
    fn test_two_char_names() {
        assert_eq!(index_to_name(53), "aa");
        assert_eq!(index_to_name(54), "ab");
        assert_eq!(index_to_name(53 + 62), "a9");
        assert_eq!(index_to_name(53 + 63), "ba");
    }

    #[test]
    fn test_names_up_to_length() {
        assert_eq!(names_up_to_length(1), 53);
        assert_eq!(names_up_to_length(2), 53 + 53 * 63);
    }

    #[test]
    fn test_no_keywords() {
        let mut mangler = Mangler::new();
        for i in 0..100 {
            let name = mangler.mangle("Test", &format!("field_{}", i));
            assert!(
                !LUA_KEYWORDS.contains(&name.as_str()),
                "Generated keyword: {}",
                name
            );
        }
    }

    #[test]
    fn test_per_type_isolation() {
        let mut mangler = Mangler::new();
        let a = mangler.mangle("ClassA", "foo");
        let b = mangler.mangle("ClassB", "foo");
        // Both get the same index (first name) since they're different types
        assert_eq!(a, b);
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
        let new_a = mangler.mangle("ClassA", "_new");
        let new_b = mangler.mangle("ClassB", "_new");
        assert_eq!(new_a, new_b, "_new should have same mangled name across types");

        let vals_a = mangler.mangle("EnumA", "_values");
        let vals_b = mangler.mangle("EnumB", "_values");
        assert_eq!(vals_a, vals_b, "_values should have same mangled name across types");
    }
}
