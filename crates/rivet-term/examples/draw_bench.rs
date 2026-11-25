//! How long a frame of a full, colourful screen takes, and how fast output streams in while
//! frames are drawn. `cargo run --release -p rivet-term --example draw_bench`
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, FontId, Rect, Vec2};
use rivet_term::{Session, Size, Spawn, Style, TermColors, view};

fn main() {
    let ctx = egui::Context::default();
    let raw = || egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    // A screen of short coloured words, like `ls --color` or a compiler's output.
    let script = r#"i=0; while [ $i -lt 400 ]; do j=0; line=""; while [ $j -lt 20 ]; do line="$line\033[3$((j%8))mword$j\033[0m "; j=$((j+1)); done; printf "$line\n"; i=$((i+1)); done; sleep 30"#;
    let spawn = Spawn {
        program: Some(("sh".into(), vec!["-c".into(), script.into()])),
        scrollback: 10_000,
        ..Default::default()
    };
    let size = Size {
        cols: 80,
        lines: 24,
        cell_w: 8,
        cell_h: 16,
    };
    let mut s = Session::spawn(&spawn, size, None).unwrap();
    let style = Style {
        font: FontId::monospace(13.0),
        colors: TermColors::from_theme(
            Color32::from_gray(20),
            Color32::from_gray(220),
            Color32::LIGHT_BLUE,
        ),
        cursor: Default::default(),
        padding: 8.0,
        bold_is_bright: true,
        visual_bell: false,
        radius: 6.0,
    };
    let id = egui::Id::unique("bench");
    let frame = |s: &mut Session| {
        let out = ctx.run_ui(raw(), |ui| {
            let r = ui.max_rect();
            view::show(ui, id, r, s, &style);
        });
        std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
    };
    frame(&mut s);
    std::thread::sleep(Duration::from_millis(1500));
    for _ in 0..20 {
        frame(&mut s);
    }
    let n = 300;
    let t = Instant::now();
    for _ in 0..n {
        frame(&mut s);
    }
    let per = t.elapsed() / n;
    println!("frame (full screen, static): {per:?}");
    drop(s);

    // Streaming: how long `cat`-like output takes while the window draws as fast as it can.
    let big = std::env::temp_dir().join(format!("rivet-term-bench-{}.txt", std::process::id()));
    let mut text = String::new();
    for i in 0..200_000 {
        text.push_str(&format!(
            "\x1b[3{}mline {i:>7}\x1b[0m the quick brown fox jumps over the lazy dog 0123456789\n",
            i % 8
        ));
    }
    std::fs::write(&big, &text).unwrap();
    let spawn = Spawn {
        program: Some((
            "sh".into(),
            vec![
                "-c".into(),
                format!(
                    "cat {}; printf '\\033]2;DONE\\007'; sleep 30",
                    big.display()
                ),
            ],
        )),
        scrollback: 10_000,
        ..Default::default()
    };
    let mut s = Session::spawn(&spawn, size, None).unwrap();
    let t = Instant::now();
    let mut frames = 0;
    'outer: loop {
        frame(&mut s);
        frames += 1;
        for n in s.poll(0.0, &style.colors) {
            if matches!(n, rivet_term::Notice::Title(ref x) if x == "DONE") {
                break 'outer;
            }
        }
        if t.elapsed() > Duration::from_secs(60) {
            println!("timed out");
            break;
        }
    }
    println!(
        "stream {} MB: {:?} over {frames} frames",
        text.len() / 1_000_000,
        t.elapsed()
    );
    let _ = std::fs::remove_file(big);
}
