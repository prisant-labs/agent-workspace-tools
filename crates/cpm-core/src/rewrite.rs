#[derive(Debug, Clone, PartialEq)]
pub struct RewriteRule {
    pub find: String,
    pub replace: String,
}

pub fn build_path_rules(old_abs: &str, new_abs: &str) -> Vec<RewriteRule> {
    let esc = |p: &str| p.replace('\\', "\\\\"); // JSON-escaped backslash form
    let fwd = |p: &str| p.replace('\\', "/"); // forward-slash form
    let (oe, ne) = (esc(old_abs), esc(new_abs));
    let (of, nf) = (fwd(old_abs), fwd(new_abs));
    vec![
        RewriteRule {
            find: format!(r#""cwd":"{oe}""#),
            replace: format!(r#""cwd":"{ne}""#),
        },
        RewriteRule {
            find: format!("{oe}\\\\"),
            replace: format!("{ne}\\\\"),
        },
        RewriteRule {
            find: format!("{of}/"),
            replace: format!("{nf}/"),
        },
    ]
}

pub fn anchored_rewrite(text: &str, rules: &[RewriteRule]) -> (String, usize) {
    let mut out = text.to_string();
    let mut total = 0usize;
    for r in rules {
        if r.find.is_empty() {
            continue;
        }
        total += out.matches(&r.find).count();
        out = out.replace(&r.find, &r.replace);
    }
    (out, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_replaces_literally() {
        let rules = vec![RewriteRule {
            find: "a/b/".into(),
            replace: "x/y/".into(),
        }];
        let (out, n) = anchored_rewrite("a/b/1 a/b/2 a/bc", &rules);
        assert_eq!(n, 2); // a/bc must NOT match (no trailing slash)
        assert_eq!(out, "x/y/1 x/y/2 a/bc");
    }
}
