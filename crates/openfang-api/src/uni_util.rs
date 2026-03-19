/*
 * @Author             : Felix
 * @Email              : 307253927@qq.com
 * @Date               : 2026-03-19 14:08:38
 * @LastEditors        : Felix
 * @LastEditTime       : 2026-03-19 14:18:13
 */

use openfang_types::config::openfang_home_dir;
use std::path::Path;

/// Check if the path is in the home directory.
pub fn is_in_home_dir<P: AsRef<Path>>(path: &P) -> bool {
    let canonical_home = match openfang_home_dir().canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    let canonical_path = match path.as_ref().canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    canonical_home.starts_with(canonical_path)
}
