use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

/// 指定されたパスのシェルスクリプトを実行します。
///
/// # 引数
/// * `src` - 実行するシェルスクリプトのパス。
///
/// # 戻り値
/// `Result<(), Option<i32>>` - スクリプトが正常に実行された場合は `Ok(())`、
/// エラーが発生した場合はエラーコードを含む `Err(Option<i32>)` を返します。
pub fn exec_bash_script(src: &PathBuf) -> Result<(), Option<i32>> {
    // スクリプトが存在するか確認
    if !src.exists() {
        eprintln!("Error: Script not found at {:?}", src);
        return Err(None); // ファイルが見つからないエラーコード
    }

    // スクリプトが実行可能か確認 (Unix系システムの場合)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = src.metadata().map_err(|e| {
            eprintln!("Error getting script metadata: {}", e);
            2 // メタデータ取得エラー
        })?;
        let permissions = metadata.permissions();
        if permissions.mode() & 0o111 == 0 {
            eprintln!(
                "Error: Script {:?} is not executable. Trying to add execute permission.",
                src
            );
            // 実行権限を追加
            let mut perms = metadata.permissions();
            perms.set_mode(perms.mode() | 0o100); // ユーザーの実行権限を追加
            std::fs::set_permissions(src, perms).map_err(|e| {
                eprintln!("Error setting execute permission: {}", e);
                3 // 権限設定エラー
            })?;
            eprintln!("Execute permission added to {:?}", src);
        }
    }

    println!("Executing script: {:?}", src);
    io::stdout().flush().unwrap(); // 出力をフラッシュ

    let result = Command::new("bash")
        .arg(src)
        .status()
        .expect("Failed to execute");
    if result.success() {
        Ok(())
    } else {
        Err(result.code())
    }
}

fn main() {}
