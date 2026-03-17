pub fn float_min(a: f64, b: f64) -> f64 {
    // if a.lt(&b) {
    //     return a;
    // }
    // b
    a.min(b)
}

pub fn binary_search(vector: &[(u64, u64)], value: u64) -> Option<(u64, u64)> {
    match vector.binary_search_by(|x| x.0.cmp(&value)) {
        Ok(idx) => {
            // Found exact match, return next if exists
            if idx + 1 < vector.len() {
                Some(vector[idx + 1])
            } else {
                None
            }
        }
        Err(idx) => {
            // Not found, idx is the insertion point for value
            if idx < vector.len() {
                Some(vector[idx])
            } else {
                None
            }
        }
    }
}