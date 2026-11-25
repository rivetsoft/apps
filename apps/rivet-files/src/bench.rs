//! `rivet --bench [FOLDER…]` — time the work behind opening a folder, without a window.
//! Prints only timings and counts (no file names), median of several runs.

use std::path::PathBuf;
use std::time::Instant;

use crate::icons::{self, IconTheme};
use crate::mime::MimeDb;
use crate::model::{Location, Tab};
use crate::settings::{Settings, SortKey};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

pub fn run(folders: Vec<PathBuf>) {
    const RUNS: usize = 7;
    let folders = if folders.is_empty() { vec![crate::places::home()] } else { folders };

    let t = Instant::now();
    let db = MimeDb::load();
    let mime_ms = ms(t);

    let theme = icons::detect_system_theme();
    let t = Instant::now();
    let mut it = IconTheme::new(&theme);
    let theme_ms = ms(t);

    println!("Rivet benchmark ({} runs each, median)", RUNS);
    println!("  file-type database load      {:>8.1} ms", mime_ms);
    println!("  icon theme index ({theme})  {:>8.1} ms", theme_ms);

    let mut settings = Settings::default();
    for dir in folders {
        let mut tab = Tab::new(Location::Dir(dir.clone()));
        let mut load = Vec::new();
        let mut sort_name = Vec::new();
        let mut sort_type = Vec::new();
        let mut mixed_name = Vec::new();
        let mut mixed_type = Vec::new();
        for _ in 0..RUNS {
            let t = Instant::now();
            tab.reload(&db, &settings);
            load.push(ms(t));
            // In the order the folder lists them (already sorted when the files were made in
            // name order, as test folders usually are), then scrambled, like a real folder.
            for (order, name, ty) in [(false, &mut sort_name, &mut sort_type), (true, &mut mixed_name, &mut mixed_type)] {
                if order {
                    tab.scramble();
                }
                settings.sort = SortKey::Name;
                let t = Instant::now();
                tab.resort(&db, &settings);
                name.push(ms(t));
                settings.sort = SortKey::Type;
                let t = Instant::now();
                tab.resort(&db, &settings);
                ty.push(ms(t));
            }
        }
        settings.sort = SortKey::Name;
        // Icon lookup for every item: first pass is cold (disk), second is cached.
        let names: Vec<Vec<String>> = tab.entries.iter().map(|e| db.icon_names(&e.mime)).collect();
        let t = Instant::now();
        let mut found = 0;
        for n in &names {
            let refs: Vec<&str> = n.iter().map(|s| s.as_str()).collect();
            if it.find(&refs, 48).is_some() {
                found += 1;
            }
        }
        let cold = ms(t);
        let t = Instant::now();
        for n in &names {
            let refs: Vec<&str> = n.iter().map(|s| s.as_str()).collect();
            let _ = it.find(&refs, 48);
        }
        let warm = ms(t);
        // Gathering the selection, which the status bar and Peek do every frame.
        let mut sel_one = Vec::new();
        let mut sel_all = Vec::new();
        if let Some(&mid) = tab.visible.get(tab.visible.len() / 2) {
            tab.selected = std::iter::once(tab.entries[mid].path.clone()).collect();
            for _ in 0..RUNS {
                let t = Instant::now();
                std::hint::black_box(tab.selected_entries());
                sel_one.push(ms(t));
            }
            tab.selected = tab.visible.iter().map(|&i| tab.entries[i].path.clone()).collect();
            for _ in 0..RUNS {
                let t = Instant::now();
                std::hint::black_box(tab.selected_entries());
                sel_all.push(ms(t));
            }
            tab.selected.clear();
        }
        let n = tab.entries.len();
        println!("\n  folder with {n} items");
        if !sel_one.is_empty() {
            println!("    find the selection, one / all {:>8.3} ms / {:.1} ms", median(sel_one), median(sel_all));
        }
        println!("    read + inspect every item    {:>8.1} ms  ({:.1} µs per item)", median(load.clone()), median(load) * 1000.0 / n.max(1) as f64);
        println!("    sort by name, folder order   {:>8.2} ms", median(sort_name));
        println!("    sort by name, scrambled      {:>8.2} ms", median(mixed_name));
        println!("    sort by type, folder order   {:>8.2} ms", median(sort_type));
        println!("    sort by type, scrambled      {:>8.2} ms", median(mixed_type));
        println!("    find icons, first time       {:>8.1} ms  ({found}/{n} found)", cold);
        println!("    find icons, cached           {:>8.2} ms", warm);
    }
}
