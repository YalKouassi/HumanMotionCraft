# The product name

The name is set once, in `crates/ac-core/src/product.rs`:

| Constant | Value | Used for |
|---|---|---|
| `NAME` / `product_name!()` | `HumanMotionCraft` | window title, start-up error, Built-in tab tooltip, MCP instructions, fixture gallery title, the data folder `%APPDATA%/<NAME>/library` |
| `ID` / `product_id!()` | `humanmotioncraft` | MCP `serverInfo.name`, file formats (`<ID>.clip`, `.pose`, `.rig`), the glTF `scenes[0].extras` key, temp folder names |
| `FORMER` | `("AnimationCraft", "animationcraft")` | names the product had before. Files with a former id still load and are saved under `ID`. A library in a former data folder is still found |

Rust code never spells the name. The usage text in `ac-app` comes from `env!("CARGO_BIN_NAME")`.
`tools/drive_ui.py` reads the binary name from `crates/ac-app/Cargo.toml`. Crate docs and Cargo
descriptions say "the app", not the name.

## History
- **AnimationCraft → HumanMotionCraft** (2026-10-28). Still under the old name:
  - `tests/fixtures/golden/*.json` say `"format": "animationcraft.golden"`. They were written by
    the earlier Python prototype (cf-animate, not public) and load under `FORMER`; they stay as
    they are.
  - Outside this repo: the user library in `%APPDATA%/AnimationCraft/library` (still used until
    `%APPDATA%/HumanMotionCraft/library` exists: move it to finish), and any MCP server added by
    hand as `animationcraft` (`claude mcp add humanmotioncraft …`).

## Renaming again
1. In `product.rs`: set `product_name!()` and `product_id!()` to the new name, and push the old
   (name, id) pair onto the front of `FORMER`.
2. Rename the `[[bin]]` in `crates/ac-app/Cargo.toml` and the server key in `.mcp.json`.
3. Rewrite the `format` lines in `assets/rig/*.json` and `assets/models/MANIFEST.json`. Rewrite
   the samples with `ac samples --bless`. (The built-in library's clips and poses use the earlier
   prototype's `cf-animate.*` formats and have nothing to change.)
4. Change the prose in `README.md`, `AGENTS.md`, `ROADMAP.md`, `docs/`, `training/`, the `ac-feature`
   skill and `NOTICE` (`git grep -i -I "<old name>"` finds every place).

## The `ac` prefix
`ac` stands for AnimationCraft, the old name. It appears in the crate names (`ac-core` …
`ac-app`, hundreds of references), the CLI binary `ac` / `ac.exe`, the dev binary `ac-review`, the
env vars `AC_FFMPEG`, `AC_BLESS_VISUAL`, `AC_LOG`, `AC_CONTROL_TIMEOUT`, `AC_TEXTMOTION_DIR`,
`AC_TRAIN_OUT`, `AC_AI_MODEL` and `AC_AI_GOLDEN`, the MCP resources `ac://…`, the skill
`ac-feature`, the Python packages `ac_train` and `ac_rig`, the formats `ac-inbetweener` and
`ac-rig-data`, and the temp prefix `ac-drive-`. Users never see the crate names. Renaming the
prefix is a separate decision. If you rename the CLI binary, avoid `mc`: Midnight Commander and
the MinIO client already use it.

## Other products' names
`cargo xtask names` (part of `cargo xtask check` and CI) fails when another product's name appears
in a file or a file path. The list in `xtask/src/names.rs` holds only
their 64-bit FNV-1a hashes, so the guard never spells what it guards against. It matches whole
words: text is split at anything that is not a letter and at lower-to-upper case changes, then
lower-cased. A base model we credit because its licence asks for it may be named only in the
credits (`ATTRIBUTION.md`, `NOTICE`, `README.md`, `assets/SOURCES.json`, the model cards under
`assets/models/`). Product names may appear nowhere. To add a name, `cargo xtask names --hash <word>` prints the number to add.
