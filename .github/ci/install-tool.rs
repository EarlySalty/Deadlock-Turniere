//! Install only the reviewed Linux/amd64 release bytes in tools.tsv, without a shell.
use std::{
    env,
    error::Error,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: install-tool NAME DESTINATION".into());
    }
    let row = include_str!("tools.tsv")
        .lines()
        .find(|line| line.split('\t').next() == Some(args[1].as_str()))
        .ok_or("tool is not in the reviewed allowlist")?;
    let fields: Vec<_> = row.split('\t').collect();
    if fields.len() != 3 || fields[1].len() != 64 {
        return Err("invalid tool manifest".into());
    }
    let dest = PathBuf::from(&args[2]);
    fs::create_dir_all(&dest)?;
    let archive = dest.join(format!("{}.tgz", args[1]));
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "3",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--output",
        ])
        .arg(&archive)
        .arg(fields[2])
        .status()?;
    if !status.success() {
        return Err("release download failed".into());
    }
    let hash = Command::new("sha256sum").arg(&archive).output()?;
    if !hash.status.success()
        || String::from_utf8(hash.stdout)?.split_whitespace().next() != Some(fields[1])
    {
        return Err("release SHA256 mismatch; refusing execution".into());
    }
    let listing = Command::new("tar").arg("-tzf").arg(&archive).output()?;
    if !listing.status.success() {
        return Err("invalid release archive".into());
    }
    let listing = String::from_utf8(listing.stdout)?;
    let matches: Vec<_> = listing
        .lines()
        .filter(|path| path.rsplit('/').next() == Some(args[1].as_str()))
        .collect();
    if matches.len() != 1 {
        return Err("release must contain exactly one requested executable".into());
    }
    let partial = dest.join(format!("{}.partial", args[1]));
    let output = fs::File::create(&partial)?;
    let status = Command::new("tar")
        .arg("-xOzf")
        .arg(&archive)
        .arg("--")
        .arg(matches[0])
        .stdout(Stdio::from(output))
        .status()?;
    if !status.success() {
        return Err("release extraction failed".into());
    }
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o755))?;
    fs::rename(partial, dest.join(&args[1]))?;
    println!("Verified {}: {}", args[1], fields[1]);
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
