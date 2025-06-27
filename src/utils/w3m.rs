use std::io;
use std::process::Command;

pub fn w3m(args: &[&str]) -> io::Result<String> {
    // let w3m_bin = include_bytes!("../../lib/bin/w3m");
    let output = Command::new("w3m").args(args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("w3m failed: {}", String::from_utf8_lossy(&output.stderr)),
        ))
    }
}
