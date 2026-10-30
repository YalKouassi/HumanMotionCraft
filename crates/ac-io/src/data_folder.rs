//! After a rename, the app's data left under a former name's folder moves to the current one:
//! `%APPDATA%/<former>/library` (and `models`) becomes `%APPDATA%/<NAME>/library`, once.
//! [`crate::library::app_library_dir`] still finds a library that couldn't be moved.

use std::path::Path;

use ac_core::product;

/// What to move: the library and the app's own models.
const MOVED: [&str; 2] = ["library", "models"];

/// Move a former name's data under `base` (the `APPDATA` folder) to the current name's.
/// `None`: nothing to move (no former folder, or the current library exists already).
/// `Some(Ok(note))`: moved, and what to tell the person. `Some(Err(e))`: it couldn't be moved
/// (a file open in another program); the former folder stays in use.
pub fn move_former(base: &Path) -> Option<Result<String, String>> {
    let current = base.join(product::NAME);
    if current.join("library").exists() {
        return None;
    }
    let (former, _) = product::FORMER
        .iter()
        .find(|f| base.join(f.0).join("library").is_dir())?;
    let old = base.join(former);
    let mut moved = Vec::new();
    for what in MOVED {
        let (from, to) = (old.join(what), current.join(what));
        if !from.exists() || to.exists() {
            continue;
        }
        let done = std::fs::create_dir_all(&current)
            .and_then(|()| std::fs::rename(&from, &to))
            .map_err(|e| {
                format!(
                    "couldn't move your {what} from {} to {}: {e}; it is used where it is",
                    from.display(),
                    to.display()
                )
            });
        if let Err(e) = done {
            return Some(Err(e));
        }
        moved.push(what);
    }
    // the former folder goes once it is empty
    if std::fs::read_dir(&old).is_ok_and(|mut d| d.next().is_none()) {
        let _ = std::fs::remove_dir(&old);
    }
    Some(Ok(format!(
        "moved your {} from {} to {}",
        moved.join(" and "),
        old.display(),
        current.display()
    )))
}

/// [`move_former`] on the `APPDATA` folder (none: nothing to do).
pub fn move_former_data() -> Option<Result<String, String>> {
    move_former(Path::new(&std::env::var_os("APPDATA")?))
}
