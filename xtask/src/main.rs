//! 仓库维护工具：配置复制、前端 dist 准备、测试入口、portable zip 打包。
//!
//! 把发布与构建流程集中在 Rust 里，避免同一逻辑在 PowerShell / bash / Makefile
//! 里各写一份，保证 Windows、Linux 开发机和 CI 上行为一致。由 Makefile 和
//! `.github/workflows/release.yml` 调用。

use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::Command,
};

use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

mod audit_usage;
mod logs_analyze;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
struct PackageOptions {
    version: String,
    arch: String,
    release_dir: PathBuf,
    out_dir: PathBuf,
    upx: bool,
    include_sdl2_dll: bool,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("package-portable") => package_portable(parse_package_args(args.collect())?),
        Some("prepare-frontend") => prepare_frontend_cmd(),
        Some("copy-config") => copy_config_cmd(parse_copy_config_args(args.collect())?),
        Some("prepare-exe") => prepare_exe_cmd(parse_prepare_exe_args(args.collect())?),
        Some("clean-dist") => clean_dist(),
        Some("test") => run_nextest(["--workspace"], None),
        Some("test-core") => run_nextest(["-p", "bitcat-core"], None),
        Some("test-app") => run_nextest(["-p", "bitcat-app"], None),
        Some("test-fast") => run_nextest(
            ["-p", "bitcat-core", "-E", "not test(/prop_/)"],
            Some(("PROPTEST_CASES", "32")),
        ),
        Some("audit-usage") => {
            let mut days = 0u32;
            let mut iter = args.peekable();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--days" => {
                        days = iter
                            .next()
                            .and_then(|value| value.parse().ok())
                            .ok_or("audit-usage --days 需要一个数字")?;
                    }
                    "-h" | "--help" => {
                        print_help();
                        return Ok(());
                    }
                    _ => return Err(format!("unknown audit-usage option: {arg}").into()),
                }
            }
            audit_usage::run(days)
        }
        Some("logs") => match args.next().as_deref() {
            Some("analyze") => logs_analyze::run(&logs_analyze::parse_args(args.collect())?),
            _ => Err("用法: xtask logs analyze <bitcat-diagnostics-xxx.zip>".into()),
        },
        Some("-h") | Some("--help") | None => {
            print_help();
            Ok(())
        }
        Some(cmd) => Err(format!("unknown xtask command: {cmd}").into()),
    }
}

fn parse_copy_config_args(args: Vec<String>) -> Result<PathBuf> {
    let mut out_dir = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out-dir" => out_dir = Some(PathBuf::from(required_value(&mut iter, "--out-dir")?)),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown copy-config option: {arg}").into()),
        }
    }

    out_dir.ok_or_else(|| "copy-config requires --out-dir <path>".into())
}

fn copy_config_cmd(out_dir: PathBuf) -> Result<()> {
    let repo_root = env::current_dir()?;
    copy_config(
        &repo_root.join("config"),
        &repo_root.join(out_dir).join("config"),
    )?;
    Ok(())
}

fn parse_prepare_exe_args(args: Vec<String>) -> Result<PathBuf> {
    let mut out_dir = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out-dir" => out_dir = Some(PathBuf::from(required_value(&mut iter, "--out-dir")?)),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown prepare-exe option: {arg}").into()),
        }
    }

    out_dir.ok_or_else(|| "prepare-exe requires --out-dir <path>".into())
}

fn prepare_exe_cmd(out_dir: PathBuf) -> Result<()> {
    let repo_root = env::current_dir()?;
    let out_dir = repo_root.join(out_dir);
    let bitcat_exe = out_dir.join("bitcat.exe");
    if !bitcat_exe.is_file() {
        return Err(format!("executable not found: {}", bitcat_exe.display()).into());
    }
    println!("Prepared executable: {}", bitcat_exe.display());
    Ok(())
}

fn prepare_frontend_cmd() -> Result<()> {
    let repo_root = env::current_dir()?;
    let frontend_dir = repo_root.join("app").join("frontend");
    let dist_dir = repo_root.join("app").join("frontend-dist");

    if !frontend_dir.is_dir() {
        return Err(format!("frontend directory not found: {}", frontend_dir.display()).into());
    }

    remove_if_exists(&dist_dir)?;
    fs::create_dir_all(&dist_dir)?;

    for entry in fs::read_dir(&frontend_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
            fs::copy(&path, dist_dir.join(entry.file_name()))?;
        }
    }

    for dir in ["assets", "css", "js"] {
        copy_dir_recursive(&frontend_dir.join(dir), &dist_dir.join(dir))?;
    }
    copy_dir_recursive(
        &frontend_dir.join("__fixtures__").join("pets"),
        &dist_dir.join("__fixtures__").join("pets"),
    )?;

    let (file_count, total_bytes) = dir_stats(&dist_dir)?;
    let size_mb = total_bytes as f64 / 1024.0 / 1024.0;
    println!(
        "Prepared frontend dist: {} ({file_count} files, {size_mb:.2} MB)",
        dist_dir.display()
    );
    Ok(())
}

fn clean_dist() -> Result<()> {
    let repo_root = env::current_dir()?;
    for entry in fs::read_dir(&repo_root)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("bitcat-") && name.ends_with(".zip") {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    if !src.is_dir() {
        return Err(format!(
            "frontend runtime asset directory not found: {}",
            src.display()
        )
        .into());
    }

    fs::create_dir_all(dest)?;
    let mut entries = fs::read_dir(src)?.collect::<std::result::Result<Vec<_>, io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, dest_path)?;
        }
    }
    Ok(())
}

fn dir_stats(dir: &Path) -> Result<(usize, u64)> {
    let mut file_count = 0;
    let mut total_bytes = 0;
    collect_dir_stats(dir, &mut file_count, &mut total_bytes)?;
    Ok((file_count, total_bytes))
}

fn collect_dir_stats(dir: &Path, file_count: &mut usize, total_bytes: &mut u64) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_stats(&path, file_count, total_bytes)?;
        } else {
            *file_count += 1;
            *total_bytes += entry.metadata()?.len();
        }
    }
    Ok(())
}

/// 运行测试：优先 cargo-nextest，未安装时回退到 `cargo test`。
///
/// nextest 不属于 Rust 工具链，新机器（尤其是非 Windows 开发机）常常没装。
/// Makefile 声称“nextest → 回退 cargo test”，这里把该承诺落实，避免
/// `make test-core` 在没装 nextest 的环境上直接失败。
fn run_nextest<const N: usize>(args: [&str; N], env_var: Option<(&str, &str)>) -> Result<()> {
    copy_config_cmd(PathBuf::from("core"))?;

    let use_nextest = nextest_available();
    let has_filter = args.iter().any(|arg| arg.starts_with("-E"));

    let mut cmd = Command::new("cargo");
    if use_nextest {
        cmd.args(["nextest", "run"]);
        cmd.args(args);
    } else {
        let mut note =
            String::from("xtask: 未检测到 cargo-nextest，回退到 `cargo test`（安装：cargo install cargo-nextest --locked）");
        if has_filter {
            note.push_str("\nxtask: `cargo test` 不支持 nextest 的 -E 过滤表达式，已丢弃该参数；proptest 用例数仍由 PROPTEST_CASES 控制");
        }
        println!("{note}");
        cmd.arg("test");
        let mut skip_next = false;
        for arg in args {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg == "-E" {
                skip_next = true;
                continue;
            }
            if arg.starts_with("-E") {
                continue;
            }
            cmd.arg(arg);
        }
    }
    if let Some((key, value)) = env_var {
        cmd.env(key, value);
    }

    let runner = if use_nextest {
        "cargo nextest"
    } else {
        "cargo test"
    };
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("{runner} failed with status: {status}").into());
    }
    Ok(())
}

/// 探测 cargo-nextest 是否可用（一次轻量级子进程调用）。
fn nextest_available() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_package_args(args: Vec<String>) -> Result<PackageOptions> {
    let mut version = None;
    let mut target = None;
    let mut arch = String::from("x64");
    let mut release_dir = None;
    let mut out_dir = PathBuf::from(".");
    let mut upx = false;
    let mut include_sdl2_dll = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--version" => version = Some(required_value(&mut iter, "--version")?),
            "--target" => target = Some(required_value(&mut iter, "--target")?),
            "--arch" => arch = required_value(&mut iter, "--arch")?,
            "--release-dir" => {
                release_dir = Some(PathBuf::from(required_value(&mut iter, "--release-dir")?))
            }
            "--out-dir" => out_dir = PathBuf::from(required_value(&mut iter, "--out-dir")?),
            "--upx" => upx = true,
            "--include-sdl2-dll" => include_sdl2_dll = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown package-portable option: {arg}").into()),
        }
    }

    let version = match version {
        Some(version) => version,
        None => git_describe().unwrap_or_else(|| String::from("dev")),
    };
    let release_dir = release_dir.unwrap_or_else(|| {
        target
            .as_ref()
            .map(|target| PathBuf::from("target").join(target).join("release"))
            .unwrap_or_else(|| PathBuf::from("target/release"))
    });

    Ok(PackageOptions {
        version,
        arch,
        release_dir,
        out_dir,
        upx,
        include_sdl2_dll,
    })
}

fn required_value(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    iter.next()
        .ok_or_else(|| format!("{name} requires a value").into())
}

fn package_portable(options: PackageOptions) -> Result<()> {
    let repo_root = env::current_dir()?;
    let release_dir = repo_root.join(&options.release_dir);
    let exe = release_dir.join("bitcat.exe");
    if !exe.is_file() {
        return Err(format!("executable not found: {}", exe.display()).into());
    }

    if options.upx {
        run_upx(&exe)?;
    }

    let out_dir = repo_root.join(&options.out_dir);
    fs::create_dir_all(&out_dir)?;

    let portable_name = format!(
        "bitcat-{}-windows-{}-portable",
        options.version, options.arch
    );
    let portable_dir = repo_root.join(&portable_name);
    let zip_path = out_dir.join(format!("{portable_name}.zip"));

    remove_if_exists(&portable_dir)?;
    remove_if_exists(&zip_path)?;

    fs::create_dir_all(&portable_dir)?;
    fs::copy(&exe, portable_dir.join("bitcat.exe"))?;

    copy_config(&repo_root.join("config"), &portable_dir.join("config"))?;

    if options.include_sdl2_dll {
        let sdl2_dll = release_dir.join("SDL2.dll");
        if !sdl2_dll.is_file() {
            return Err(format!("SDL2.dll not found: {}", sdl2_dll.display()).into());
        }
        fs::copy(&sdl2_dll, portable_dir.join("SDL2.dll"))?;
    }

    zip_dir(&portable_dir, &zip_path)?;
    fs::remove_dir_all(&portable_dir)?;

    let size_kb = fs::metadata(&zip_path)?.len() as f64 / 1024.0;
    println!("Portable ZIP: {} ({size_kb:.1} KB)", zip_path.display());
    Ok(())
}

/// 把 config/ 复制到目标目录：顶层收 *.yml，子目录（如 dances/*.yaml）递归收 yml/yaml。
///
/// 曾经只扫顶层 yml，dances/ 整个子目录被落下——便携包用户因此拿不到内置舞蹈
/// （配合 dance.rs 里 bundled_dance_dir 的编译期路径问题，发布包的内置舞蹈完全失效）。
fn copy_config(src: &Path, dest: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_config(&path, &dest.join(entry.file_name()))?;
            continue;
        }
        let is_yaml = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yml") | Some("yaml")
        );
        if is_yaml {
            fs::copy(&path, dest.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn zip_dir(src_dir: &Path, zip_path: &Path) -> Result<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    add_dir_to_zip(src_dir, src_dir, &mut zip, options)?;
    zip.finish()?;
    Ok(())
}

fn add_dir_to_zip(
    root: &Path,
    dir: &Path,
    zip: &mut ZipWriter<File>,
    options: SimpleFileOptions,
) -> Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<std::result::Result<Vec<_>, io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(root, &path, zip, options)?;
        } else {
            let relative = path.strip_prefix(root)?;
            let name = relative.to_string_lossy().replace('\\', "/");
            zip.start_file(name, options)?;
            let mut file = File::open(&path)?;
            io::copy(&mut file, zip)?;
        }
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn run_upx(exe: &Path) -> Result<()> {
    let status = Command::new("upx")
        .args(["--best", "--lzma"])
        .arg(exe)
        .status()
        .map_err(|err| {
            format!("failed to run upx; install it with `winget install UPX.UPX`: {err}")
        })?;

    if !status.success() {
        return Err(format!("upx failed with status: {status}").into());
    }
    Ok(())
}

fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!version.is_empty()).then_some(version)
}

fn print_help() {
    println!(
        "\
xtask commands:
  package-portable [options]
  copy-config --out-dir <path>
  prepare-exe --out-dir <path>
  clean-dist
  test | test-core | test-app | test-fast
  audit-usage [--days N]        F2 功能使用审计：读 ~/.bitcat/logs/ 埋点出报告
  logs analyze <zip>            分析其他设备导出的诊断包（多源合并 + chat 链路）

package-portable options:
  --version <value>          Release version/tag. Defaults to git describe.
  --target <triple>          Cargo target triple, e.g. x86_64-pc-windows-msvc.
  --arch <value>             Artifact arch label. Defaults to x64.
  --release-dir <path>       Release directory. Defaults to target[/target]/release.
  --out-dir <path>           Output directory. Defaults to current directory.
  --upx                      Compress the release executable with UPX before packaging.
  --include-sdl2-dll         Include SDL2.dll for dynamic SDL2 builds.
"
    );
}
