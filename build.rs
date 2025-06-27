use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

/// 指定されたパスのシェルスクリプトを実行します。
///
/// # 引数
/// * `src` - 実行するシェルスクリプトのパス。
///
/// # 戻り値
/// `Result<(), std::io::Error>` - スクリプトが正常に実行された場合は `Ok(())`、
/// エラーが発生した場合はエラーコードを含む `Err(std::io::Error)` を返します。
pub fn exec_bash_script(src: &PathBuf) -> Result<(), std::io::Error> {
    // スクリプトが存在するか確認
    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Error: Script not found at {:?}", src),
        ));
    }

    // スクリプトが実行可能か確認 (Unix系システムの場合)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = src.metadata()?;
        let permissions = metadata.permissions();
        if permissions.mode() & 0o111 == 0 {
            eprintln!(
                "Error: Script {:?} is not executable. Trying to add execute permission.",
                src
            );
            // 実行権限を追加
            let mut perms = metadata.permissions();
            perms.set_mode(perms.mode() | 0o100); // ユーザーの実行権限を追加
            std::fs::set_permissions(src, perms)?;
            eprintln!("Execute permission added to {:?}", src);
        }
    }

    println!("Executing script: {:?}", src);
    io::stdout().flush().unwrap();

    let result = Command::new("bash")
        .arg(src)
        .status()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to execute: {}", e)))?;
    if result.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Script exited with code: {:?}", result.code()),
        ))
    }
}

fn main() -> Result<(), std::io::Error> {
    exec_bash_script(&PathBuf::from("build/w3m.sh"))
}
