//! Measures the MVP gate: list a folder of 10,000 files.
//! Run: `cargo run --release -p nexus-core --example bench_listing [folder]`
//! Without a folder argument it creates a temporary one.

use nexus_core::model::{ListOptions, SortKey, SortOptions};
use std::time::Instant;

fn main() {
    let arg = std::env::args().nth(1);
    let tmp;
    let dir = match arg {
        Some(d) => d,
        None => {
            tmp = std::env::temp_dir().join("nexus-bench-10k");
            if !tmp.exists() {
                std::fs::create_dir_all(&tmp).unwrap();
                for i in 0..10_000 {
                    std::fs::write(tmp.join(format!("Báo cáo {i}.xlsx")), b"").unwrap();
                }
            }
            tmp.display().to_string()
        }
    };
    for key in [SortKey::Name, SortKey::Modified] {
        let opts = ListOptions { show_hidden: true, sort: SortOptions { key, descending: false, folders_first: true } };
        let mut times = vec![];
        let mut n = 0;
        for _ in 0..10 {
            let t = Instant::now();
            n = nexus_core::list_directory(dir.clone(), opts).unwrap().entries.len();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!("{n} entries, sort {key:?}: best {:.1} ms, median {:.1} ms", times[0], times[5]);
    }
}
