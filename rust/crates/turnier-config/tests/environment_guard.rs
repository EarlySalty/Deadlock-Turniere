//! Wächter für den Rust-Laufzeitpfad, einschließlich Code nach Testmodulen.
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|name| name == "test")
    })
}

struct Guard<'a> {
    file: &'a str,
    violations: Vec<&'static str>,
}
impl Guard<'_> {
    fn secret_module(&self) -> bool {
        self.file == "turnier-config/src/secrets.rs"
    }
    fn binary(&self) -> bool {
        matches!(
            self.file,
            "turnier-bot/src/main.rs" | "turnier-observer/src/bin/agent.rs"
        )
    }
}

impl<'ast> Visit<'ast> for Guard<'_> {
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
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if !test_only(&item.attrs) {
            visit::visit_item_impl(self, item);
        }
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn names(tree: &syn::UseTree, out: &mut Vec<String>) {
            match tree {
                syn::UseTree::Path(path) => {
                    out.push(path.ident.to_string());
                    names(&path.tree, out);
                }
                syn::UseTree::Name(name) => out.push(name.ident.to_string()),
                syn::UseTree::Rename(name) => out.push(name.ident.to_string()),
                syn::UseTree::Group(group) => group.items.iter().for_each(|tree| names(tree, out)),
                syn::UseTree::Glob(_) => {}
            }
        }
        if test_only(&item.attrs) || self.secret_module() {
            return;
        }
        let mut parts = Vec::new();
        names(&item.tree, &mut parts);
        if parts.iter().any(|name| forbidden(name)) {
            self.violations.push("ENV-Import oder Alias");
        }
        visit::visit_item_use(self, item);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func {
            let parts: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let name = parts.join("::");
            let cli = self.binary() && name == "std::env::args_os";
            let dsn =
                self.file == "turnier-db/src/pool.rs" && name == "dl_central_db::dsn_from_env";
            let test_guard = self.file == "turnier-api/src/test_mode.rs"
                && name == "std::env::var"
                && call.args.len() == 1
                && matches!(call.args.first(), Some(syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(key), .. }))
                    if matches!(key.value().as_str(), "CENTRAL_TEST_DSN" | "DEADLOCK_CENTRAL_DSN" | "TURNIER_TEST_DB_CONFIRM"));
            if cli || dsn || test_guard {
                for arg in &call.args {
                    self.visit_expr(arg);
                }
                return;
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if !self.secret_module()
            && path
                .segments
                .iter()
                .any(|s| forbidden(&s.ident.to_string()))
        {
            self.violations.push("ENV-Leser oder Konfigurationsbrücke");
        }
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.is_ident("env") || mac.path.is_ident("option_env") {
            let metadata = self.file == "turnier-observer/src/bin/agent.rs"
                && mac.path.is_ident("env")
                && syn::parse2::<syn::LitStr>(mac.tokens.clone())
                    .is_ok_and(|s| s.value() == "CARGO_PKG_VERSION");
            if !metadata {
                self.violations
                    .push("Kompilierzeit-ENV als Betriebskonfiguration");
            }
            return;
        }
        visit::visit_macro(self, mac);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if !self.secret_module() && forbidden(&call.method.to_string()) {
            self.violations.push("ENV-Methode oder Exportbrücke");
        }
        visit::visit_expr_method_call(self, call);
    }
}
fn forbidden(name: &str) -> bool {
    matches!(
        name,
        "env"
            | "envs"
            | "vars"
            | "vars_os"
            | "var"
            | "var_os"
            | "getenv"
            | "setenv"
            | "putenv"
            | "unsetenv"
            | "set_var"
            | "remove_var"
            | "from_env"
            | "dsn_from_env"
            | "from_default_env"
            | "try_from_default_env"
            | "dotenv"
            | "dotenvy"
            | "Environment"
    )
}
fn inspect(file: &str, text: &str) -> Vec<&'static str> {
    let parsed = syn::parse_file(text).expect("Rust-Quelle für ENV-Wächter parsbar");
    let mut guard = Guard {
        file,
        violations: Vec::new(),
    };
    guard.visit_file(&parsed);
    guard.violations
}
fn walk(path: &Path, sources: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, sources);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            sources.push(path);
        }
    }
}
#[test]
fn runtime_environment_access_is_explicitly_allowlisted() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            walk(&src, &mut sources);
        }
    }
    assert!(sources.len() > 50);
    for source in sources {
        let name = source
            .strip_prefix(crates)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let issues = inspect(&name, &std::fs::read_to_string(&source).unwrap());
        assert!(issues.is_empty(), "{name}: {issues:?}");
    }
}
#[test]
fn guard_detects_aliases_dynamic_keys_and_code_after_test_modules() {
    for source in [
        "#[cfg(test)] mod tests {} fn runtime() { std::env::var(\"PORT\"); }",
        "use std::env::var as read; fn runtime() { read(\"PORT\"); }",
        "use std::env; fn runtime(key: &str) { env::var(format!(\"{key}_FILE\")); }",
        "fn runtime() { option_env!(\"BACKEND_PORT\"); }",
        "fn runtime() { std::env::vars(); }",
        "fn runtime() { dotenvy::dotenv(); }",
        "fn runtime() { command.env(\"BACKEND_PORT\", \"1\"); }",
    ] {
        assert!(!inspect("turnier-api/src/new.rs", source).is_empty());
    }
    assert!(inspect(
        "turnier-api/src/new.rs",
        "#[cfg(test)] mod tests { fn t() { std::env::var(\"TEST\"); } } fn runtime() {} "
    )
    .is_empty());
    assert!(!inspect(
        "turnier-api/src/test_mode.rs",
        "fn guard() { std::env::var(format!(\"BACKEND_PORT\")); }"
    )
    .is_empty());
}
