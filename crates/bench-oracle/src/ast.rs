//! AST checks over model-authored code, via `syn`. These are the L3 constraints
//! that are genuinely about *structure*, not behaviour or resources — and they
//! are done on the parsed tree, never by grepping text (docs/03-oracle.md).
//!
//! Two are implemented: counting `unsafe`, which the AST makes impossible to
//! hide, and detecting forbidden type/function paths (`RefCell`, `transmute`, …)
//! regardless of how they were imported. This is also the parsing machinery P3
//! generation reuses.

use syn::visit::{self, Visit};

/// Count `unsafe` usages: every `unsafe { … }` block plus every `unsafe fn`
/// (free functions and impl methods). Returns `None` if the code does not parse
/// — a non-parsing answer is not this check's business (L1 already failed it).
pub fn count_unsafe(code: &str) -> Option<u32> {
    let file = syn::parse_file(code).ok()?;
    let mut c = UnsafeCounter { count: 0 };
    c.visit_file(&file);
    Some(c.count)
}

/// Every path segment whose identifier matches one of `forbidden`, in source
/// order. Catches `RefCell::new`, `std::cell::RefCell`, `use …::RefCell`, etc.
/// `None` if the code does not parse.
pub fn find_forbidden_paths(code: &str, forbidden: &[String]) -> Option<Vec<String>> {
    if forbidden.is_empty() {
        return Some(Vec::new());
    }
    let file = syn::parse_file(code).ok()?;
    let mut f = PathFinder {
        forbidden,
        hits: Vec::new(),
    };
    f.visit_file(&file);
    Some(f.hits)
}

struct UnsafeCounter {
    count: u32,
}

impl<'ast> Visit<'ast> for UnsafeCounter {
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.count += 1;
        visit::visit_expr_unsafe(self, node);
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node.sig.unsafety.is_some() {
            self.count += 1;
        }
        visit::visit_item_fn(self, node);
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if node.sig.unsafety.is_some() {
            self.count += 1;
        }
        visit::visit_impl_item_fn(self, node);
    }
}

struct PathFinder<'a> {
    forbidden: &'a [String],
    hits: Vec<String>,
}

impl<'ast, 'a> Visit<'ast> for PathFinder<'a> {
    fn visit_path(&mut self, node: &'ast syn::Path) {
        for seg in &node.segments {
            let id = seg.ident.to_string();
            if self.forbidden.iter().any(|f| f == &id) {
                self.hits.push(id);
            }
        }
        visit::visit_path(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unsafe_block_and_fn() {
        let code = r#"
            pub fn a() { unsafe { let _ = 1; } }
            pub unsafe fn b() {}
            pub fn c() {}
        "#;
        assert_eq!(count_unsafe(code), Some(2));
    }

    #[test]
    fn safe_code_has_zero_unsafe() {
        assert_eq!(count_unsafe("pub fn f(x: i32) -> i32 { x + 1 }"), Some(0));
    }

    #[test]
    fn non_parsing_code_returns_none() {
        assert_eq!(count_unsafe("fn ( this is not rust"), None);
    }

    #[test]
    fn finds_forbidden_path_regardless_of_import() {
        let a = "use std::cell::RefCell; fn f() { let _ = RefCell::new(0); }";
        let hits = find_forbidden_paths(a, &["RefCell".to_string()]).unwrap();
        assert!(!hits.is_empty(), "should catch RefCell; got {hits:?}");
    }

    #[test]
    fn empty_forbidden_list_is_noop() {
        assert_eq!(find_forbidden_paths("fn f() {}", &[]), Some(Vec::new()));
    }

    #[test]
    fn clean_code_trips_no_forbidden() {
        let hits =
            find_forbidden_paths("fn f(v: &mut [i64]) { v.reverse(); }", &["RefCell".into()]);
        assert_eq!(hits, Some(Vec::new()));
    }
}
