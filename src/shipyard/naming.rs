const ID_LEN: usize = 8;

fn normalize_class_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Short file token: 8 hex chars. Not sequential, so deletes leave no holes.
pub fn allocate_id<'a>(taken: impl IntoIterator<Item = &'a str>) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    let taken: std::collections::HashSet<String> = taken.into_iter().map(str::to_string).collect();
    for _ in 0..64 {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(1);
        let mix = t
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(u128::from(std::process::id()))
            .wrapping_add(u128::from(COUNTER.fetch_add(1, Ordering::Relaxed)));
        let id = format!("{:08x}", (mix ^ (mix >> 32) ^ (mix >> 64)) as u32);
        if id.len() == ID_LEN
            && !taken.contains(&id)
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return id;
        }
    }
    format!("s{:07x}", std::process::id() & 0x0fff_ffff)
}

/// Default class label for a hull, unique among `taken` names.
pub fn unique_class_name<'a>(hull: &str, taken: impl IntoIterator<Item = &'a str>) -> String {
    let taken: std::collections::HashSet<String> =
        taken.into_iter().map(normalize_class_name).collect();
    let base = format!("Basic {hull}");
    if !taken.contains(&normalize_class_name(&base)) {
        return base;
    }
    for n in 2u32..1000 {
        let name = format!("{base} {n}");
        if !taken.contains(&normalize_class_name(&name)) {
            return name;
        }
    }
    format!("{base} {}", allocate_id(std::iter::empty()))
}

pub fn names_collide(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && normalize_class_name(a) == normalize_class_name(b)
}

pub fn is_generated_class_name(name: &str, hull_names: &[&str]) -> bool {
    let name = name.trim();
    for hull in hull_names {
        let base = format!("Basic {hull}");
        if name == base {
            return true;
        }
        if let Some(rest) = name.strip_prefix(&format!("{base} ")) {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}
