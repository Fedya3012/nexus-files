use super::*;
use std::fs;
use std::time::{Duration, Instant};

struct Fixture {
    _data: tempfile::TempDir,
    root: tempfile::TempDir,
    engine: Arc<Engine>,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    // Under $HOME so the freedesktop trash is on the same filesystem in CI.
    let root = tempfile::tempdir_in(dirs::home_dir().unwrap()).unwrap();
    let engine = Engine::new(JsonStore::open(data.path()).unwrap());
    Fixture { _data: data, root, engine }
}

fn wait(engine: &Engine, id: u64) -> JobStatus {
    let start = Instant::now();
    loop {
        let s = engine.job(id).unwrap();
        if is_finished(s.state) {
            return s;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "job {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

#[test]
fn copy_reports_progress_and_undo_removes_copies() {
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("src/album")).unwrap();
    fs::write(r.join("src/album/a.jpg"), vec![1u8; 100_000]).unwrap();
    fs::write(r.join("src/b.txt"), b"hello").unwrap();
    fs::create_dir(r.join("dst")).unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Copy {
            sources: vec![s(&r.join("src/album")), s(&r.join("src/b.txt"))],
            destination: s(&r.join("dst")),
            conflict: ConflictPolicy::KeepBoth,
        })
        .unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.state, JobState::Completed, "{:?}", st.errors);
    assert_eq!(st.bytes_total, 100_005);
    assert_eq!(st.bytes_done, 100_005);
    assert!(r.join("dst/album/a.jpg").exists() && r.join("dst/b.txt").exists());
    assert!(st.undoable);
    assert_eq!(f.engine.undo_title().unwrap(), "Copy 2 mục sang ".to_string() + &s(&r.join("dst")));

    let u = f.engine.undo_last().unwrap();
    assert_eq!(wait(&f.engine, u).state, JobState::Completed);
    assert!(!r.join("dst/album").exists() && !r.join("dst/b.txt").exists());
    assert!(r.join("src/album/a.jpg").exists(), "sources untouched");
    assert!(f.engine.undo_title().is_none());
}

#[test]
fn copy_into_same_folder_keeps_both() {
    let f = fixture();
    let r = f.root.path();
    fs::write(r.join("a.txt"), b"1").unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Copy { sources: vec![s(&r.join("a.txt"))], destination: s(r), conflict: ConflictPolicy::Replace })
        .unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.result_paths, vec![s(&r.join("a (2).txt"))]);
}

#[test]
fn move_and_undo_restores_original_locations() {
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("a/inner")).unwrap();
    fs::write(r.join("a/inner/x.txt"), b"x").unwrap();
    fs::write(r.join("f.txt"), b"f").unwrap();
    fs::create_dir(r.join("b")).unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Move { sources: vec![s(&r.join("a")), s(&r.join("f.txt"))], destination: s(&r.join("b")), conflict: ConflictPolicy::KeepBoth })
        .unwrap();
    assert_eq!(wait(&f.engine, id).state, JobState::Completed);
    assert!(r.join("b/a/inner/x.txt").exists() && !r.join("a").exists());
    let u = f.engine.undo_last().unwrap();
    assert_eq!(wait(&f.engine, u).state, JobState::Completed);
    assert!(r.join("a/inner/x.txt").exists() && r.join("f.txt").exists());
    assert!(!r.join("b/a").exists());
}

#[test]
fn moving_folder_into_itself_is_refused() {
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("a/sub")).unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Move { sources: vec![s(&r.join("a"))], destination: s(&r.join("a/sub")), conflict: ConflictPolicy::KeepBoth })
        .unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.state, JobState::Failed);
    assert!(r.join("a/sub").exists());
}

#[test]
fn move_within_same_folder_is_a_noop() {
    let f = fixture();
    let r = f.root.path();
    fs::write(r.join("a.txt"), b"1").unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Move { sources: vec![s(&r.join("a.txt"))], destination: s(r), conflict: ConflictPolicy::KeepBoth })
        .unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.skipped, 1);
    assert!(r.join("a.txt").exists() && !r.join("a (2).txt").exists());
}

#[test]
fn rename_validates_and_undoes() {
    let f = fixture();
    let r = f.root.path();
    fs::write(r.join("old.txt"), b"1").unwrap();
    fs::write(r.join("taken.txt"), b"2").unwrap();
    assert!(f.engine.execute(FileOperation::Rename { path: s(&r.join("old.txt")), new_name: "bad/name".into() }).is_err());
    assert!(matches!(
        f.engine.execute(FileOperation::Rename { path: s(&r.join("old.txt")), new_name: "taken.txt".into() }),
        Err(NexusError::Io { .. })
    ));
    let st = f.engine.execute(FileOperation::Rename { path: s(&r.join("old.txt")), new_name: "Mới.txt".into() }).unwrap();
    assert_eq!(st.result_paths, vec![s(&r.join("Mới.txt"))]);
    let u = f.engine.undo_last().unwrap();
    wait(&f.engine, u);
    assert!(r.join("old.txt").exists() && !r.join("Mới.txt").exists());
}

#[test]
fn new_folder_gets_unique_name_and_undo_removes_it() {
    let f = fixture();
    let r = f.root.path();
    let a = f.engine.execute(FileOperation::NewFolder { parent: s(r), name: "Thư mục mới".into() }).unwrap();
    let b = f.engine.execute(FileOperation::NewFolder { parent: s(r), name: "Thư mục mới".into() }).unwrap();
    assert_eq!(a.result_paths[0], s(&r.join("Thư mục mới")));
    assert_eq!(b.result_paths[0], s(&r.join("Thư mục mới (2)")));
    wait(&f.engine, f.engine.undo_last().unwrap());
    assert!(!r.join("Thư mục mới (2)").exists());
    assert!(r.join("Thư mục mới").exists());
}

#[test]
fn trash_and_restore() {
    let f = fixture();
    let r = f.root.path();
    fs::write(r.join("bỏ đi.txt"), b"data").unwrap();
    let id = f.engine.enqueue(FileOperation::Trash { paths: vec![s(&r.join("bỏ đi.txt"))] }).unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.state, JobState::Completed, "{:?}", st.errors);
    assert!(!r.join("bỏ đi.txt").exists());
    let u = wait(&f.engine, f.engine.undo_last().unwrap());
    assert_eq!(u.state, JobState::Completed, "{:?}", u.errors);
    assert_eq!(fs::read(r.join("bỏ đi.txt")).unwrap(), b"data");
}

#[test]
fn replace_trashes_old_copy_and_undo_brings_it_back() {
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::create_dir_all(r.join("dst")).unwrap();
    fs::write(r.join("src/a.txt"), b"new").unwrap();
    fs::write(r.join("dst/a.txt"), b"old").unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Copy { sources: vec![s(&r.join("src/a.txt"))], destination: s(&r.join("dst")), conflict: ConflictPolicy::Replace })
        .unwrap();
    assert_eq!(wait(&f.engine, id).state, JobState::Completed);
    assert_eq!(fs::read(r.join("dst/a.txt")).unwrap(), b"new");
    let u = wait(&f.engine, f.engine.undo_last().unwrap());
    assert_eq!(u.state, JobState::Completed, "{:?}", u.errors);
    assert_eq!(fs::read(r.join("dst/a.txt")).unwrap(), b"old");
}

#[test]
fn skip_policy_leaves_existing() {
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::create_dir_all(r.join("dst")).unwrap();
    fs::write(r.join("src/a.txt"), b"new").unwrap();
    fs::write(r.join("dst/a.txt"), b"old").unwrap();
    let id = f
        .engine
        .enqueue(FileOperation::Copy { sources: vec![s(&r.join("src/a.txt"))], destination: s(&r.join("dst")), conflict: ConflictPolicy::Skip })
        .unwrap();
    let st = wait(&f.engine, id);
    assert_eq!(st.skipped, 1);
    assert_eq!(st.state, JobState::CompletedWithErrors);
    assert_eq!(fs::read(r.join("dst/a.txt")).unwrap(), b"old");
}

#[test]
fn journal_survives_restart() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    fs::write(r.join("a.txt"), b"1").unwrap();
    {
        let e = Engine::new(JsonStore::open(data.path()).unwrap());
        e.execute(FileOperation::Rename { path: s(&r.join("a.txt")), new_name: "b.txt".into() }).unwrap();
    }
    let e = Engine::new(JsonStore::open(data.path()).unwrap());
    assert_eq!(e.history(10).len(), 1);
    wait(&e, e.undo_last().unwrap());
    assert!(r.join("a.txt").exists());
}

#[test]
fn many_files_copy_without_loss() {
    // Part of the MVP gate "không mất dữ liệu trong bộ test copy/move".
    let f = fixture();
    let r = f.root.path();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::create_dir_all(r.join("dst")).unwrap();
    for i in 0..500 {
        fs::write(r.join(format!("src/f{i}.bin")), vec![(i % 251) as u8; 1000 + i]).unwrap();
    }
    let sources: Vec<String> = (0..500).map(|i| s(&r.join(format!("src/f{i}.bin")))).collect();
    let id = f.engine.enqueue(FileOperation::Copy { sources: sources.clone(), destination: s(&r.join("dst")), conflict: ConflictPolicy::KeepBoth }).unwrap();
    assert_eq!(wait(&f.engine, id).state, JobState::Completed);
    let id = f.engine.enqueue(FileOperation::Move { sources, destination: s(&r.join("dst")), conflict: ConflictPolicy::KeepBoth }).unwrap();
    assert_eq!(wait(&f.engine, id).state, JobState::Completed);
    for i in 0..500 {
        let expect = vec![(i % 251) as u8; 1000 + i];
        assert_eq!(fs::read(r.join(format!("dst/f{i}.bin"))).unwrap(), expect);
        assert_eq!(fs::read(r.join(format!("dst/f{i} (2).bin"))).unwrap(), expect);
    }
}
