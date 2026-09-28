//! fractional sort keys: strings over base-36 digits, compared bytewise, so
//! an item can be moved between two siblings by giving it a key in between,
//! without renumbering anything else.

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE: usize = DIGITS.len();

fn digit(c: u8) -> usize {
    DIGITS
        .iter()
        .position(|&d| d == c)
        .expect("sort key contains a non base-36 digit")
}

fn is_valid(key: &str) -> bool {
    !key.is_empty() && !key.ends_with('0') && key.bytes().all(|c| DIGITS.contains(&c))
}

/// a key strictly between `a` and `b` (`None` meaning unbounded on that side).
/// keys that aren't valid sort keys (e.g. from another client) are treated as
/// unbounded, which may reorder that one neighbour but never panics.
pub fn key_between(a: Option<&str>, b: Option<&str>) -> String {
    let a = a.filter(|k| is_valid(k)).unwrap_or("");
    let b = b.filter(|k| is_valid(k) && *k > a);
    midpoint(a.as_bytes(), b.map(str::as_bytes))
}

// midpoint of two base-36 fractions `0.a` and `0.b`, a < b (b = None = 1)
fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // shared prefix is copied as is
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let prefix = String::from_utf8(b[..n].to_vec()).unwrap();
            return prefix + &midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..]));
        }
    }

    let da = a.first().map(|&c| digit(c)).unwrap_or(0);
    let db = b.and_then(|b| b.first()).map(|&c| digit(c)).unwrap_or(BASE);
    if db - da > 1 {
        return (DIGITS[(da + db).div_ceil(2)] as char).to_string();
    }
    // adjacent digits: go one level deeper
    match b {
        Some(b) if b.len() > 1 => (b[0] as char).to_string(),
        _ => (DIGITS[da] as char).to_string() + &midpoint(a.get(1..).unwrap_or(&[]), None),
    }
}

/// `n` increasing keys spread evenly over the key space, used for a fresh
/// upload so later inserts rarely need longer keys.
pub fn evenly_spaced(n: usize) -> Vec<String> {
    let mut width = 1;
    while BASE.pow(width as u32) <= n + 1 {
        width += 1;
    }
    let space = BASE.pow(width as u32);
    (1..=n)
        .map(|i| {
            let mut value = i * space / (n + 1);
            let mut digits = vec![b'0'; width];
            for slot in digits.iter_mut().rev() {
                *slot = DIGITS[value % BASE];
                value /= BASE;
            }
            while digits.last() == Some(&b'0') {
                digits.pop();
            }
            String::from_utf8(digits).unwrap()
        })
        .collect()
}

/// assigns keys to an ordered list of siblings, keeping as many `existing`
/// keys as possible (longest increasing run of them) and minting new
/// keys only for the rest, so a single move changes a single key.
pub fn assign(existing: &[Option<String>]) -> Vec<String> {
    if existing.iter().all(Option::is_none) {
        return evenly_spaced(existing.len());
    }

    let keep = longest_increasing(existing);
    let mut out: Vec<String> = Vec::with_capacity(existing.len());
    for (i, key) in existing.iter().enumerate() {
        if keep[i] {
            out.push(key.clone().unwrap());
            continue;
        }
        let prev = out.last().map(String::as_str);
        let next = (i + 1..existing.len())
            .find(|&j| keep[j])
            .and_then(|j| existing[j].as_deref());
        out.push(key_between(prev, next));
    }
    out
}

// marks the members of a longest strictly-increasing subsequence of the
// valid keys (patience sorting, O(n log n))
fn longest_increasing(keys: &[Option<String>]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new(); // index of smallest tail per length
    let mut prev: Vec<Option<usize>> = vec![None; keys.len()];

    for (i, key) in keys.iter().enumerate() {
        let Some(key) = key.as_deref().filter(|k| is_valid(k)) else {
            continue;
        };
        let pos = tails.partition_point(|&t| keys[t].as_deref().unwrap() < key);
        prev[i] = pos.checked_sub(1).map(|p| tails[p]);
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }

    let mut keep = vec![false; keys.len()];
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        keep[i] = true;
        cur = prev[i];
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_sorted(keys: &[String]) {
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "{:?} not < {:?}", w[0], w[1]);
        }
        for k in keys {
            assert!(is_valid(k), "invalid key {k:?}");
        }
    }

    #[test]
    fn between_is_strictly_between() {
        let cases = [
            (None, None),
            (None, Some("i")),
            (Some("i"), None),
            (Some("a"), Some("b")),
            (Some("a"), Some("a1")),
            (Some("az"), Some("b")),
            (Some("z"), None),
            (Some("zzz"), None),
            (None, Some("01")),
            (Some("a0001"), Some("a0002")),
        ];
        for (a, b) in cases {
            let k = key_between(a, b);
            assert!(is_valid(&k), "{a:?}..{b:?} gave invalid {k:?}");
            if let Some(a) = a {
                assert!(a < k.as_str(), "{a:?} !< {k:?}");
            }
            if let Some(b) = b {
                assert!(k.as_str() < b, "{k:?} !< {b:?}");
            }
        }
    }

    #[test]
    fn repeated_inserts_stay_ordered() {
        // append, prepend and bisect many times
        let mut keys = vec![key_between(None, None)];
        for i in 0..300 {
            match i % 3 {
                0 => keys.push(key_between(keys.last().map(String::as_str), None)),
                1 => keys.insert(0, key_between(None, Some(&keys[0]))),
                _ => {
                    let mid = keys.len() / 2;
                    let k = key_between(Some(&keys[mid - 1]), Some(&keys[mid]));
                    keys.insert(mid, k);
                }
            }
            assert_sorted(&keys);
        }
    }

    #[test]
    fn evenly_spaced_is_sorted_and_short() {
        for n in [0, 1, 2, 35, 36, 37, 1000, 5000] {
            let keys = evenly_spaced(n);
            assert_eq!(keys.len(), n);
            assert_sorted(&keys);
            assert!(keys.iter().all(|k| k.len() <= 3), "n={n}");
        }
    }

    #[test]
    fn assign_keeps_keys_and_only_rekeys_moved_items() {
        let k: Vec<String> = evenly_spaced(4); // a b c d
        // move d to the front: only d gets a new key
        let existing = vec![
            Some(k[3].clone()),
            Some(k[0].clone()),
            Some(k[1].clone()),
            Some(k[2].clone()),
        ];
        let out = assign(&existing);
        assert_sorted(&out);
        assert_eq!(&out[1..], &k[..3]);

        // new items in the middle / end get keys between their neighbours
        let existing = vec![Some(k[0].clone()), None, Some(k[1].clone()), None];
        let out = assign(&existing);
        assert_sorted(&out);
        assert_eq!(
            (out[0].as_str(), out[2].as_str()),
            (k[0].as_str(), k[1].as_str())
        );
    }

    #[test]
    fn assign_tolerates_foreign_and_duplicate_keys() {
        let existing = vec![
            Some("".to_string()),
            Some("B".to_string()),
            Some("m".to_string()),
            Some("m".to_string()),
            Some("a0".to_string()),
        ];
        assert_sorted(&assign(&existing));
    }
}
