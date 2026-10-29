//! Reading the setup's log (`ac_textmotion::setup::feed`): the numbered step lines give the
//! progress, and the last other line is what the panel shows under it (a download's progress
//! bar, which rewrites its line with `\r`, or the error the setup ended with).

use ac_textmotion::setup::{Progress, feed};

fn read(chunks: &[&str]) -> (Progress, String) {
    let (mut p, mut tail) = (Progress::default(), String::new());
    for c in chunks {
        feed(&mut p, &mut tail, c);
    }
    (p, tail)
}

#[test]
fn a_numbered_line_is_the_step_and_clears_the_last_line() {
    let (p, _) = read(&[
        "Installing Kimodo into C:\\x\n\n== [1/6] Kimodo source\n  $ git clone\n",
        "\n== [3/6] Text encoder (Llama 3 8B Instruct and the LLM2Vec adapters)\n",
    ]);
    assert_eq!((p.step, p.steps), (3, 6));
    assert_eq!(
        p.label,
        "Text encoder (Llama 3 8B Instruct and the LLM2Vec adapters)"
    );
    assert_eq!(p.last, "");
}

#[test]
fn a_progress_bar_shows_its_latest_state_and_a_line_split_across_reads_is_whole() {
    let (p, tail) = read(&[
        "== [4/6] Kimodo weights\nmodel.safetensors:  10%|#  | 100M/1.0G\rmodel.safe",
        "tensors:  55%|#####  | 550M/1.0G\r",
    ]);
    assert_eq!(p.step, 4);
    assert_eq!(p.last, "model.safetensors:  55%|#####  | 550M/1.0G");
    assert_eq!(tail, "");
}

#[test]
fn an_unfinished_line_waits_and_colour_codes_are_dropped() {
    let (p, tail) = read(&["== [2/6] Python environment\n\u{1b}[1mBuilt\u{1b}[0m kimodo\nhalf a li"]);
    assert_eq!(p.last, "Built kimodo");
    assert_eq!(tail, "half a li");
}

#[test]
fn the_error_the_setup_ends_with_is_the_last_line() {
    let (p, _) = read(&[
        "== [2/6] Python environment (torch, Kimodo, motion correction)\n",
        "`uv pip install` failed (exit code 2); the lines above say why\n",
    ]);
    assert_eq!(
        (p.step, p.label.as_str()),
        (2, "Python environment (torch, Kimodo, motion correction)")
    );
    assert_eq!(
        p.last,
        "`uv pip install` failed (exit code 2); the lines above say why"
    );
}

#[test]
fn a_line_that_only_looks_numbered_is_an_ordinary_line() {
    let (p, _) =
        read(&["== [1/6] Kimodo source\n== [x/6] not a step\n== stopped: the app ended the install\n"]);
    assert_eq!(p.step, 1);
    assert_eq!(p.last, "== stopped: the app ended the install");
}

#[test]
fn the_done_line_says_it_finished() {
    let (p, _) =
        read(&["== [6/6] Check: the helper loads the model\nKimodo is installed in D:/hmc\n\n== done\n"]);
    assert!(p.done);
    assert_eq!((p.step, p.last.as_str()), (6, "Kimodo is installed in D:/hmc"));
}
