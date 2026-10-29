//! The product's name, in one place. Code says `product::NAME` / `product::ID` (or the macros, where
//! a `const` needs `concat!`), never the name itself, so a rename is an edit here plus the places
//! that can't read a constant (Cargo names, `.mcp.json`, docs): `docs/naming.md` lists them.

/// The name people see: window title, messages, the user data folder.
#[macro_export]
macro_rules! product_name {
    () => {
        "HumanMotionCraft"
    };
}

/// The machine id (lower case, no spaces): MCP server name, file-format prefix, temp folders.
#[macro_export]
macro_rules! product_id {
    () => {
        "humanmotioncraft"
    };
}

pub const NAME: &str = product_name!();
pub const ID: &str = product_id!();

/// What the product was called before, newest first, as (`NAME`, `ID`). Files carrying a former
/// id still load (they are rewritten with `ID` when saved), and a library left in a former data
/// folder is still found. On a rename, push the old pair here.
pub const FORMER: &[(&str, &str)] = &[("AnimationCraft", "animationcraft")];

/// The `format` value files of `kind` are written with: `format("clip")` is `"<ID>.clip"`.
pub fn format(kind: &str) -> String {
    format!("{ID}.{kind}")
}

/// Whether a file's `format` value names `kind` under the current id or a former one.
pub fn is_format(format: &str, kind: &str) -> bool {
    names_kind(
        format,
        kind,
        std::iter::once(ID).chain(FORMER.iter().map(|f| f.1)),
    )
}

fn names_kind<'a>(format: &str, kind: &str, mut ids: impl Iterator<Item = &'a str>) -> bool {
    format
        .split_once('.')
        .is_some_and(|(id, k)| k == kind && ids.any(|i| i == id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_format_round_trips() {
        assert!(is_format(&format("clip"), "clip"));
        assert!(!is_format(&format("clip"), "pose"));
        assert!(!is_format("clip", "clip"));
        assert!(!is_format("someone-else.clip", "clip"));
    }

    #[test]
    fn former_ids_are_read() {
        let ids = || ["newname", "oldname"].into_iter();
        assert!(names_kind("oldname.clip", "clip", ids()));
        assert!(names_kind("newname.clip", "clip", ids()));
        assert!(!names_kind("oldname.pose", "clip", ids()));
    }
}
