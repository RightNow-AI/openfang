/*
 * @Author             : Felix
 * @Email              : 307253927@qq.com
 * @Date               : 2026-03-19 14:08:38
 * @LastEditors        : Felix
 * @LastEditTime       : 2026-03-19 15:39:00
 */

use openfang_types::config::openfang_home_dir;
use std::path::Path;

/// Check if the path is in the home directory.
/// if the path is in the home directory, return true.
/// if the path is not exist, return false.
pub fn is_in_home_dir<P: AsRef<Path>>(path: P) -> bool {
    let canonical_home = match openfang_home_dir().canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    let canonical_path = match path.as_ref().canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    canonical_path.starts_with(canonical_home)
}

#[test]
fn test_is_in_home_dir() {
    let home_dir = openfang_home_dir();
    println!("home_dir: {:?}", home_dir);
    assert!(is_in_home_dir(&home_dir), "Path1 is in home directory");
    assert!(!is_in_home_dir("~"), "Path2 is not in home directory");
}
