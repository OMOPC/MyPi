//! Where a file sits in its crate: the module it belongs to, and the entry file that declares it.

use std::path::{Path, PathBuf};

use super::text::{strip_attrs, strip_vis};
use crate::server::agent::tools::workspace::canonicalize_lenient;

/// One module, as the file the model just read sees it.
pub(crate) struct Module {
    /// The module the file belongs to: `web::cdp` for `web/cdp/client.rs`, the crate for `lib.rs`.
    pub chain: Vec<String>,
    /// Where the module's own files live.
    pub dir: PathBuf,
    /// The module's entry file: `mod.rs`, the dual-track `x.rs`, or `lib.rs`/`main.rs`.
    pub hub: Option<PathBuf>,
    /// The crate root, the directory holding `Cargo.toml`.
    pub root: PathBuf,
    /// The crate's `src/`.
    pub src: PathBuf,
    /// The file that was read.
    pub file: PathBuf,
}

/// The module `file` belongs to, when it sits under a crate's `src/`.
pub(crate) fn locate(file: &Path) -> Option<Module> {
    if file.extension().and_then(|e| e.to_str()) != Some("rs") {
        return None;
    }
    let file = canonicalize_lenient(file);
    let root = crate_root(&file)?;
    let src = root.join("src");
    let rel = file.strip_prefix(&src).ok()?;
    let stem = rel.file_stem()?.to_string_lossy().into_owned();
    let parent = rel.parent().unwrap_or_else(|| Path::new(""));
    let mut chain: Vec<String> = parent
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let mut dir = src.join(parent);
    let root_hub = chain.is_empty() && (stem == "lib" || stem == "main");
    if stem != "mod" && !root_hub {
        // A dual-track hub: `x.rs` beside an `x/` directory owns the directory's files.
        let child = dir.join(&stem);
        if child.is_dir() {
            chain.push(stem.clone());
            dir = child;
        }
    }
    let hub = hub_of(&dir, &src);
    Some(Module {
        chain,
        dir,
        hub,
        root,
        src,
        file,
    })
}

/// The crate's `src/` when `dir` sits inside one.
pub(crate) fn src_of(dir: &Path) -> Option<PathBuf> {
    for root in dir.ancestors() {
        if root.join("Cargo.toml").is_file() {
            let src = root.join("src");
            return dir.starts_with(&src).then_some(src);
        }
    }
    None
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
    canonicalize_lenient(path)
}

/// Does this text declare a module that lives in its own file? An inline `mod x { … }` and a
/// `#[cfg]`-hidden declaration both stay out: the first has no file, the second no answer.
pub(crate) fn declares_modules(text: &str) -> bool {
    text.lines().any(|line| {
        let line = strip_vis(strip_attrs(line.trim_start()));
        line.strip_prefix("mod ").is_some_and(|rest| {
            let rest = rest.trim_end();
            rest.ends_with(';') && !rest.contains('{')
        })
    })
}

pub(super) fn chain_of(file: &Path, src: &Path) -> Option<Vec<String>> {
    let rel = file.strip_prefix(src).ok()?;
    let stem = rel.file_stem()?.to_string_lossy().into_owned();
    let mut chain: Vec<String> = rel
        .parent()
        .map(|parent| {
            parent
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if stem != "mod" && !(chain.is_empty() && (stem == "lib" || stem == "main")) {
        chain.push(stem);
    }
    Some(chain)
}

fn crate_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

pub(super) fn hub_of(dir: &Path, src: &Path) -> Option<PathBuf> {
    if dir == src {
        let lib = src.join("lib.rs");
        if lib.is_file() {
            return Some(lib);
        }
        let main = src.join("main.rs");
        return main.is_file().then_some(main);
    }
    let module = dir.join("mod.rs");
    if module.is_file() {
        return Some(module);
    }
    let sibling = dir.with_extension("rs");
    sibling.is_file().then_some(sibling)
}

pub(super) fn rel(module: &Module, path: &Path) -> String {
    let path = path.strip_prefix(&module.root).unwrap_or(path);
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}
