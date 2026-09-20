//! `cargo xtask fetch-toolchain` — turn a pinned recipe into a usable bundle.
//!
//! Download, verify, extract, prune, describe. Everything the application needs
//! to ship its own compiler, reproducibly, from a recipe under version control.
//!
//! Pure Rust rather than a shell script because this has to run on Windows CI,
//! where `unzip`, `zstd` and `tar` are not a given.

use crate::glob;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Recipe {
    target: String,
    triplet: String,
    gcc_version: String,
    channel: String,
    bundle_template: String,
    #[serde(rename = "package")]
    pub packages: Vec<Package>,
    prune: Prune,
    /// GPLv3 §6 Corresponding Source for the binaries this recipe ships.
    #[serde(rename = "source", default)]
    pub sources: Vec<Source>,
}

#[derive(Debug, Deserialize)]
pub struct Source {
    pub component: String,
    pub version: String,
    pub url: String,
    #[serde(default)]
    sha256: Option<String>,
    pub license: String,
    #[serde(default)]
    covers: Vec<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    subdir: String,
    file: String,
    sha256: String,
    size: u64,
}

impl Package {
    /// Derived rather than stored: the channel lives in one place, so a mirror
    /// change is a one-line edit and no entry can drift out of step with it.
    fn url(&self, channel: &str) -> String {
        format!(
            "{}/{}/{}",
            channel.trim_end_matches('/'),
            self.subdir,
            self.file
        )
    }
}

#[derive(Debug, Deserialize)]
struct Prune {
    keep: Vec<String>,
    #[serde(default)]
    drop: Vec<String>,
}

/// `cargo xtask fetch-sources` — assemble the Corresponding Source.
///
/// Redistributing GCC and binutils obliges us to offer their source from the
/// same place as the binaries. conda-forge does not host tarballs next to its
/// packages, so this gathers them into one directory to publish alongside a
/// release, with a SOURCES.md saying which binary each one covers.
/// Read and parse a target's recipe. Shared so packaging reports exactly the
/// components that were fetched, from the same source of truth.
pub fn read_recipe(root: &Path, target: &str) -> Result<Recipe> {
    let path = root.join("toolchain").join(format!("{target}.toml"));
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn sources(args: &[String]) -> Result<()> {
    let mut target = "linux-x86_64".to_string();
    let mut out: Option<PathBuf> = None;
    let mut list_only = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = it.next().context("--target needs a value")?.clone(),
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--list" => list_only = true,
            other => bail!(
                "unknown option `{other}`\n\n\
                 USAGE: cargo xtask fetch-sources [--target <name>] [--out <dir>] [--list]"
            ),
        }
    }

    let root = crate::repo_root();
    let recipe_path = root.join("toolchain").join(format!("{target}.toml"));
    let recipe: Recipe = toml::from_str(&fs::read_to_string(&recipe_path)?)
        .with_context(|| format!("parsing {}", recipe_path.display()))?;

    if recipe.sources.is_empty() {
        bail!("{} lists no [[source]] entries", recipe_path.display());
    }
    let dir = out.unwrap_or_else(|| {
        root.join("target")
            .join("corresponding-source")
            .join(&target)
    });
    fs::create_dir_all(&dir)?;

    let mut rows = Vec::new();
    for s in &recipe.sources {
        let file = s.url.rsplit('/').next().unwrap_or("source").to_string();
        let looks_like_a_file =
            file.contains('.') && !s.url.trim_end_matches('/').ends_with(&s.component);
        if list_only || !looks_like_a_file {
            println!("  {:<28} {:<24} {}", s.component, s.version, s.url);
            rows.push((s, file, false));
            continue;
        }
        let dest = dir.join(&file);
        let ok = match &s.sha256 {
            Some(want) => verified(&dest, want)?,
            None => dest.is_file(),
        };
        if ok {
            println!("  cached   {:<26} {}", s.component, file);
        } else {
            println!("  fetching {:<26} {}", s.component, file);
            download(&s.url, &dest).with_context(|| format!("downloading {}", s.url))?;
            if let Some(want) = &s.sha256 {
                if !verified(&dest, want)? {
                    let got = sha256_file(&dest)?;
                    let _ = fs::remove_file(&dest);
                    bail!("checksum mismatch for {file}\n  expected {want}\n  got      {got}");
                }
            }
        }
        rows.push((s, file, true));
    }

    let mut md = String::new();
    md.push_str("# Corresponding Source\n\n");
    md.push_str(
        "The binaries shipped with this application include GCC and GNU binutils, which are\n\
         licensed under the GNU General Public License version 3. GPLv3 section 6 requires\n\
         that the source be available from the same place as the binaries, for as long as\n\
         the binaries are distributed.\n\n\
         This directory is that source. Each entry below names what it covers.\n\n\
         Note that GPLv3 section 1 counts the build scripts as part of the Corresponding\n\
         Source, which is why the conda-forge recipes and their patches are listed: the\n\
         binaries are built from patched GCC, so the upstream tarball alone is not enough.\n\n",
    );
    md.push_str(&format!(
        "Toolchain: {} ({}), GCC {}.\n\n",
        recipe.target, recipe.triplet, recipe.gcc_version
    ));
    md.push_str("| Component | Version | Licence | Covers | File or location |\n");
    md.push_str("|---|---|---|---|---|\n");
    for (s, file, fetched) in &rows {
        let covers = if s.covers.iter().any(|c| c == "all") {
            "everything in the bundle".to_string()
        } else {
            s.covers.join(", ")
        };
        let loc = if *fetched {
            format!("`{file}`")
        } else {
            format!("<{}>", s.url)
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            s.component, s.version, s.license, covers, loc
        ));
    }
    md.push_str("\n## Checksums\n\n");
    for (s, file, fetched) in &rows {
        if let (true, Some(h)) = (*fetched, &s.sha256) {
            md.push_str(&format!(
                "- `{file}`\n  - sha256 `{h}`\n  - from <{}>\n",
                s.url
            ));
        }
    }
    let notes: Vec<_> = rows
        .iter()
        .filter_map(|(s, _, _)| s.note.as_ref().map(|n| (s, n)))
        .collect();
    if !notes.is_empty() {
        md.push_str("\n## Notes\n\n");
        for (s, n) in notes {
            md.push_str(&format!("- **{}**: {n}\n", s.component));
        }
    }
    let md_path = dir.join("SOURCES.md");
    fs::write(&md_path, md)?;

    println!();
    println!("wrote {}", md_path.display());
    println!();
    println!("Publish this directory next to the installer, on the same server, and keep it");
    println!("there for as long as that release is downloadable.");
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let mut target = "linux-x86_64".to_string();
    let mut out: Option<PathBuf> = None;
    let mut offline = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = it.next().context("--target needs a value")?.clone(),
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--offline" => offline = true,
            other => bail!(
                "unknown option `{other}`\n\n\
                 USAGE: cargo xtask fetch-toolchain [--target <name>] [--out <dir>] [--offline]"
            ),
        }
    }

    let root = crate::repo_root();
    let recipe_path = root.join("toolchain").join(format!("{target}.toml"));
    let recipe: Recipe = toml::from_str(
        &fs::read_to_string(&recipe_path)
            .with_context(|| format!("reading {}", recipe_path.display()))?,
    )
    .with_context(|| format!("parsing {}", recipe_path.display()))?;

    let cache = root.join("target").join("toolchain-cache").join(&target);
    let staging = root.join("target").join("toolchain-staging").join(&target);
    let bundle = out.unwrap_or_else(|| root.join("target").join("toolchain").join(&target));

    fs::create_dir_all(&cache)?;
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    println!("recipe   {}", recipe_path.display());
    println!(
        "target   {} ({}, gcc {})",
        recipe.target, recipe.triplet, recipe.gcc_version
    );
    println!("packages {}", recipe.packages.len());
    println!();

    for p in &recipe.packages {
        let archive = cache.join(&p.file);
        let have = verified(&archive, &p.sha256)?;
        if !have {
            if offline {
                bail!(
                    "{} is missing or fails its checksum, and --offline was given",
                    p.file
                );
            }
            let url = p.url(&recipe.channel);
            println!(
                "  fetching {:<26} {:<10} {:>7.1} MB",
                p.name,
                p.version,
                p.size as f64 / 1e6
            );
            download(&url, &archive).with_context(|| format!("downloading {url}"))?;
            if !verified(&archive, &p.sha256)? {
                let got = sha256_file(&archive)?;
                let _ = fs::remove_file(&archive);
                bail!(
                    "checksum mismatch for {}\n  expected {}\n  got      {}",
                    p.file,
                    p.sha256,
                    got
                );
            }
        } else {
            println!(
                "  cached   {:<26} {:<10} {:>7.1} MB",
                p.name,
                p.version,
                p.size as f64 / 1e6
            );
        }
        extract_conda(&archive, &staging).with_context(|| format!("extracting {}", p.file))?;
    }

    println!();
    let staged_bytes = tree_size(&staging)?;
    println!("extracted {:.1} MB", staged_bytes as f64 / 1e6);

    // The bundle descriptor has to be in place before pruning, because the keep
    // rules name it: the bundle is not usable without it.
    let template = root.join("toolchain").join(&recipe.bundle_template);
    fs::copy(&template, staging.join("bundle.toml"))
        .with_context(|| format!("copying {}", template.display()))?;

    // `--out` is a path a human typed. Emptying it must not be able to delete
    // something that is not ours: `--out ~` would otherwise take the home
    // directory with it.
    clear_output_dir(&bundle)
        .with_context(|| format!("preparing the output directory {}", bundle.display()))?;
    let (kept, kept_bytes, dropped, dropped_bytes) = prune(&staging, &bundle, &recipe.prune)?;
    println!(
        "pruned    {:.1} MB in {kept} files (dropped {dropped} files, {:.1} MB)",
        kept_bytes as f64 / 1e6,
        dropped_bytes as f64 / 1e6
    );

    // A prune that removes the compiler itself is a silent disaster otherwise:
    // nothing else here reads bundle.toml, so nothing would notice until someone
    // tried to build with it.
    check_driver_present(&bundle, &template)?;

    let manifest = write_manifest(&bundle, &root)?;
    println!("manifest  {}", manifest.display());
    println!();
    println!("bundle    {}", bundle.display());
    println!();
    println!("Next: verify it before trusting it —");
    println!(
        "  EF77_REQUIRE_TOOLCHAIN=1 EF77_TOOLCHAIN_BUNDLE={} \\",
        bundle.display()
    );
    println!("    cargo test -p ef-testkit --test corpus");
    println!("A prune is only correct if the whole corpus still passes.");
    Ok(())
}

/// Empty a directory we are about to fill, refusing anything that does not
/// already look like ours.
///
/// A previous bundle is recognised by its `bundle.toml`. An empty or absent
/// directory is fine. Anything else is left alone and reported, because the
/// alternative is a recursive delete of whatever the caller happened to type.
fn clear_output_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        return Ok(());
    }
    if !dir.is_dir() {
        bail!("{} exists and is not a directory", dir.display());
    }
    let ours = dir.join("bundle.toml").is_file();
    let empty = fs::read_dir(dir)?.next().is_none();
    if !ours && !empty {
        bail!(
            "{} is not empty and does not look like a previous bundle \
             (no bundle.toml). Refusing to delete it; remove it yourself or \
             choose another --out.",
            dir.display()
        );
    }
    fs::remove_dir_all(dir)?;
    fs::create_dir_all(dir)?;
    Ok(())
}

// ------------------------------------------------------------------ download

/// Stream to a temporary file and rename into place.
///
/// Streaming rather than buffering because the GCC source tarball is 185 MB and
/// there is no reason to hold it in memory. The rename means an interrupted
/// download never leaves a half-file that a later run would mistake for cached.
fn download(url: &str, dest: &Path) -> Result<()> {
    let mut res = ureq::get(url).call()?;
    let tmp = dest.with_extension("part");
    {
        let mut reader = res.body_mut().with_config().limit(u64::MAX).reader();
        let mut file = fs::File::create(&tmp)?;
        std::io::copy(&mut reader, &mut file)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn verified(path: &Path, want: &str) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    Ok(sha256_file(path)? == want)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

// ------------------------------------------------------------------- extract

/// A `.conda` package is a zip holding `pkg-<name>.tar.zst` (the files) and
/// `info-<name>.tar.zst` (metadata we do not need).
fn extract_conda(archive: &Path, into: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if !(name.starts_with("pkg-") && name.ends_with(".tar.zst")) {
            continue;
        }
        let decoder = zstd::stream::read::Decoder::new(entry)?;
        let mut tar = tar::Archive::new(decoder);
        tar.set_overwrite(true);
        tar.set_preserve_permissions(true);
        tar.unpack(into)
            .with_context(|| format!("unpacking {name}"))?;
    }
    Ok(())
}

// --------------------------------------------------------------------- prune

fn prune(src: &Path, dst: &Path, rules: &Prune) -> Result<(usize, u64, usize, u64)> {
    let keeper = |rel: &str| -> bool {
        if rules.drop.iter().any(|p| glob::matches(p, rel)) {
            return false;
        }
        rules.keep.iter().any(|p| glob::matches(p, rel))
    };

    let mut kept = 0usize;
    let mut kept_bytes = 0u64;
    let mut dropped = 0usize;
    let mut dropped_bytes = 0u64;

    // Directory symlinks first. The sysroot has `usr/lib -> lib64`, and GCC
    // resolves crt1.o through `usr/lib/../lib/`. Copying files alone silently
    // loses these, and the failure only shows up at link time.
    for entry in walk(src, true)? {
        let rel = rel_str(src, &entry);
        let out = dst.join(&rel);
        fs::create_dir_all(out.parent().unwrap())?;
        copy_symlink(&entry, &out)?;
        kept += 1;
    }

    for entry in walk(src, false)? {
        let rel = rel_str(src, &entry);
        let size = fs::symlink_metadata(&entry).map(|m| m.len()).unwrap_or(0);
        if !keeper(&rel) {
            dropped += 1;
            dropped_bytes += size;
            continue;
        }
        let out = dst.join(&rel);
        fs::create_dir_all(out.parent().unwrap())?;
        if fs::symlink_metadata(&entry)?.file_type().is_symlink() {
            copy_symlink(&entry, &out)?;
        } else {
            fs::copy(&entry, &out)?;
        }
        kept += 1;
        kept_bytes += size;
    }
    Ok((kept, kept_bytes, dropped, dropped_bytes))
}

fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    let target = fs::read_link(from)?;
    if to.exists() || fs::symlink_metadata(to).is_ok() {
        let _ = fs::remove_file(to);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, to)?;
    #[cfg(windows)]
    {
        // Windows bundles do not use symlinks; copy the target instead, so the
        // tree works without developer mode or elevation. A directory symlink
        // cannot be handled that way, and silently skipping one would produce a
        // bundle that is wrong in a way nothing here would notice.
        let resolved = from.parent().unwrap_or(Path::new(".")).join(&target);
        if resolved.is_file() {
            fs::copy(&resolved, to)?;
        } else {
            bail!(
                "{} is a symlink to {}, which is not a file. Windows bundles \
                 cannot carry directory symlinks; the recipe needs to keep the \
                 real files instead.",
                from.display(),
                target.display()
            );
        }
    }
    Ok(())
}

/// Every file (or every directory symlink) under `root`, not following symlinks.
fn walk(root: &Path, dir_symlinks: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let p = entry?.path();
            let md = fs::symlink_metadata(&p)?;
            if md.file_type().is_symlink() {
                let is_dir_link = fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
                if is_dir_link {
                    if dir_symlinks {
                        out.push(p);
                    }
                } else if !dir_symlinks {
                    out.push(p);
                }
            } else if md.is_dir() {
                stack.push(p);
            } else if !dir_symlinks {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn rel_str(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn tree_size(root: &Path) -> Result<u64> {
    Ok(walk(root, false)?
        .iter()
        .filter_map(|p| fs::symlink_metadata(p).ok().map(|m| m.len()))
        .sum())
}

/// After pruning, the compiler named by `bundle.toml` must still be there.
///
/// Cheap, and it catches the whole class of keep-list mistakes that remove the
/// thing the bundle exists for. It cannot catch a *missing* dependency -- only
/// the corpus can do that, which is why the command says so.
fn check_driver_present(bundle: &Path, template: &Path) -> Result<()> {
    let text = fs::read_to_string(template)?;
    let Some(line) = text
        .lines()
        .find(|l| l.trim_start().starts_with("gfortran"))
    else {
        bail!("{} does not name a `gfortran`", template.display());
    };
    let Some(rel) = line.split('=').nth(1) else {
        bail!(
            "{}: could not read the `gfortran` entry",
            template.display()
        );
    };
    let rel = rel.trim().trim_matches('"');
    let driver = bundle.join(rel);
    if !driver.is_file() {
        bail!(
            "the prune removed the compiler itself: bundle.toml names `{rel}`, \
             which is not in the pruned tree. Check the keep rules."
        );
    }
    println!("driver    {rel}");
    Ok(())
}

// ------------------------------------------------------------------ manifest

/// A SHA-256 of every file in the bundle, embedded in the application so a
/// missing or altered compiler is detected rather than silently used.
fn write_manifest(bundle: &Path, repo: &Path) -> Result<PathBuf> {
    let mut entries: BTreeMap<String, String> = BTreeMap::new();
    for f in walk(bundle, false)? {
        if fs::symlink_metadata(&f)?.file_type().is_symlink() {
            continue; // a symlink has no content of its own
        }
        entries.insert(rel_str(bundle, &f), sha256_file(&f)?);
    }
    let mut text = String::from(
        "# Generated by `cargo xtask fetch-toolchain`. Embedded in the application,\n\
         # so a bundled file that goes missing or changes is detected rather than used.\n",
    );
    for (path, hash) in &entries {
        text.push_str(&format!("{hash}  {path}\n"));
    }
    let dest = repo
        .join("crates")
        .join("ef-gui")
        .join("assets")
        .join("toolchain-manifest.txt");
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::write(&dest, text)?;
    Ok(dest)
}
