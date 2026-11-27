//! The order a person expects names in: "photo 2" before "photo 10", case left aside. Rivet
//! Files sorts folders this way, and Rivet View steps through a folder in the same order.

/// Natural sort: "file2" before "file10", case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    natural_cmp_lower(&a.trim_start_matches('.').to_lowercase(), &b.trim_start_matches('.').to_lowercase())
}

/// `natural_cmp` for names already lowered and trimmed, without allocating: a sort makes
/// millions of these. Comparing UTF-8 bytes orders characters as comparing them would.
pub fn natural_cmp_lower(a: &str, b: &str) -> std::cmp::Ordering {
    natural_cmp_bytes(a.as_bytes(), b.as_bytes())
}

/// [`natural_cmp_lower`] on bytes.
pub fn natural_cmp_bytes(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na = &a[si..i][a[si..i].iter().take_while(|&&c| c == b'0').count()..];
            let nb = &b[sj..j][b[sj..j].iter().take_while(|&&c| c == b'0').count()..];
            let o = na.len().cmp(&nb.len()).then(na.cmp(nb));
            if o != Ordering::Equal {
                return o;
            }
        } else {
            let o = a[i].cmp(&b[j]);
            if o != Ordering::Equal {
                return o;
            }
            i += 1;
            j += 1;
        }
    }
    (i < a.len()).cmp(&(j < b.len()))
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn numbers_in_names_count_as_numbers() {
        assert_eq!(natural_cmp("photo 2.jpg", "photo 10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("b", "A"), Ordering::Greater);
        assert_eq!(natural_cmp("photo 9", "photo 10"), Ordering::Less);
    }
}
