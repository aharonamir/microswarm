use ms_tools::{fs_list, fs_read, fs_write};
use std::{fs, path::PathBuf};

fn temp_work() -> (PathBuf, PathBuf) {
    let path = std::env::temp_dir().join(format!("ms-tools-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    let work = path.join("work");
    fs::create_dir_all(&work).unwrap();
    (path, work)
}

#[test]
#[ignore = "red: T035 — first-party filesystem MCP tools are not implemented"]
fn filesystem_tools_read_list_write_and_reject_parent_or_symlink_escape() {
    let (base, work) = temp_work();
    fs::write(work.join("hello.txt"), "hello").unwrap();
    assert_eq!(fs_read(&work, "hello.txt").unwrap(), "hello");
    fs_write(&work, "new.txt", "written").unwrap();
    assert_eq!(fs::read_to_string(work.join("new.txt")).unwrap(), "written");
    assert!(
        fs_list(&work, ".")
            .unwrap()
            .contains(&"hello.txt".to_owned())
    );
    assert!(fs_read(&work, "../outside.txt").is_err());

    let outside = base.join("outside.txt");
    fs::write(&outside, "outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, work.join("link.txt")).unwrap();
    #[cfg(unix)]
    assert!(fs_read(&work, "link.txt").is_err());

    let _ = fs::remove_file(outside);
    fs::remove_dir_all(base).unwrap();
}
