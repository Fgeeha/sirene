//! ERRORH-1: explicit panic and assertion macros belong in tests only.
//!
//! Clippy checks resolved unwrap/expect methods at each production crate root. Parse source
//! here as well so assertions and debug assertions cannot reintroduce production panics.

use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};

#[derive(Default)]
struct ProductionMacros {
    forbidden: Vec<String>,
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "test")
            || (attr.path().is_ident("cfg")
                && matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string() == "test"))
    })
}

impl<'ast> Visit<'ast> for ProductionMacros {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if let Some(name) = item.path.segments.last() {
            let name = name.ident.to_string();
            if matches!(
                name.as_str(),
                "panic"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "debug_assert"
                    | "debug_assert_eq"
                    | "debug_assert_ne"
                    | "unreachable"
                    | "todo"
                    | "unimplemented"
            ) {
                self.forbidden.push(name);
            }
        }
        visit::visit_macro(self, item);
    }
}

fn sources(dir: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, paths);
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().unwrap() != "tests.rs"
        {
            paths.push(path);
        }
    }
}

#[test]
fn production_sources_do_not_panic_explicitly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut paths = Vec::new();
    for member in [
        "crates/sirene-core",
        "crates/sirene-gui",
        "crates/sirene",
        "xtask",
    ] {
        sources(&root.join(member).join("src"), &mut paths);
    }
    paths.sort();
    let mut failures = Vec::new();
    for path in paths {
        let file = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut visitor = ProductionMacros::default();
        visitor.visit_file(&file);
        if !visitor.forbidden.is_empty() {
            failures.push(format!("{}: {:?}", path.display(), visitor.forbidden));
        }
    }
    assert!(
        failures.is_empty(),
        "production panic macros:\n{}",
        failures.join("\n")
    );
}

#[test]
fn policy_distinguishes_tests_and_production_macros() {
    let file = syn::parse_file(
        r#"
        #[cfg(test)] mod tests { fn helper() { panic!("test"); } }
        #[test] fn test_at_module_scope() { assert!(true); }
        #[gpui::test] fn gui_test() { unreachable!(); }
        fn production() { std::assert_eq!(1, 1); debug_assert!(true); }
        "#,
    )
    .unwrap();
    let mut visitor = ProductionMacros::default();
    visitor.visit_file(&file);
    assert_eq!(visitor.forbidden, ["assert_eq", "debug_assert"]);
}
