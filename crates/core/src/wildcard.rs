//! `*` patterns, as the owner writes them for held paths and guarded commands.

/// Whether `text` matches `pattern`, where `*` matches any run of characters (none included) and
/// everything else matches itself.
pub fn matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let Some((first, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return remaining.is_empty();
    };
    for part in middle {
        match remaining.find(part) {
            Some(i) => remaining = remaining.get(i + part.len()..).unwrap_or_default(),
            None => return false,
        }
    }
    remaining.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_matches_any_run_and_the_rest_matches_itself() {
        assert!(matches("terraform apply*", "terraform apply -auto-approve"));
        assert!(matches("a*c", "abc"));
        assert!(matches("a*c", "ac"));
        assert!(!matches("a*c", "abcd"));
        assert!(matches("*", ""));
        assert!(!matches("terraform apply*", "echo terraform apply"));
        assert!(matches("schema.*", "schema.prisma"));
    }
}
