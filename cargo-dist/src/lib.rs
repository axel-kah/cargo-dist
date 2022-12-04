//! # cargo-dist
//!
//!

#![allow(clippy::single_match)]
#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    process::Command,
};

use camino::{Utf8Path, Utf8PathBuf};
use cargo_dist_schema::{Artifact, DistReport, Distributable, ExecutableArtifact, Release};
use flate2::{write::ZlibEncoder, Compression, GzBuilder};
use guppy::{
    graph::{
        BuildTargetId, DependencyDirection, PackageGraph, PackageMetadata, PackageSet, Workspace,
    },
    MetadataCommand, PackageId,
};
use semver::Version;
use serde::Deserialize;
use tracing::{info, warn};
use xz2::write::XzEncoder;
use zip::ZipWriter;

use errors::*;
use miette::{miette, Context, IntoDiagnostic};

pub mod errors;
#[cfg(test)]
mod tests;

/// Key in workspace.metadata or package.metadata for our config
const METADATA_DIST: &str = "dist";
/// Dir in target/ for us to build our packages in
/// NOTE: DO NOT GIVE THIS THE SAME NAME AS A PROFILE!
const TARGET_DIST: &str = "distrib";
/// The profile we will build with
const PROFILE_DIST: &str = "dist";
/// Some files we'll try to grab.
//TODO: LICENSE-* files, somehow!
const BUILTIN_FILES: &[&str] = &["README.md", "CHANGELOG.md", "RELEASES.md"];

/// The key for referring to linux as an "os"
const OS_LINUX: &str = "linux";
/// The key for referring to macos as an "os"
const OS_MACOS: &str = "macos";
/// The key for referring to windows as an "os"
const OS_WINDOWS: &str = "windows";

/// The key for referring to 64-bit x86_64 (AKA amd64) as an "cpu"
const CPU_X64: &str = "x86_64";
/// The key for referring to 32-bit x86 (AKA i686) as an "cpu"
const CPU_X86: &str = "x86";
/// The key for referring to 64-bit arm64 (AKA aarch64) as an "cpu"
const CPU_ARM64: &str = "arm64";
/// The key for referring to 32-bit arm as an "cpu"
const CPU_ARM: &str = "arm";

/// Contents of METADATA_DIST in Cargo.toml files
#[derive(Deserialize)]
pub struct DistMetadata {}

/// A unique id for a [`BuildTarget`][]
#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Debug)]
struct BuildTargetIdx(usize);

/// A unique id for a [`BuildArtifact`][]
#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Debug)]
struct BuildArtifactIdx(usize);

/// A unique id for a [`DistributableTarget`][]
#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Debug)]
struct DistributableTargetIdx(usize);

/// The graph of all work that cargo-dist needs to do on this invocation.
///
/// All work is precomputed at the start of execution because only discovering
/// what you need to do in the middle of building/packing things is a mess.
/// It also allows us to report what *should* happen without actually doing it.
struct DistGraph {
    /// The executable cargo told us to find itself at.
    cargo: String,
    /// The cargo target dir.
    target_dir: Utf8PathBuf,
    /// The root directory of the current cargo workspace.
    workspace_dir: Utf8PathBuf,
    /// cargo-dist's target dir (generally nested under `target_dir`).
    dist_dir: Utf8PathBuf,
    /// Targets we need to build
    targets: Vec<BuildTarget>,
    /// Artifacts we want to get out of targets
    artifacts: Vec<BuildArtifact>,
    /// Distributable bundles we want to build for our artifacts
    distributables: Vec<DistributableTarget>,
    /// Logical releases that distributable bundles are grouped under
    releases: Vec<ReleaseTarget>,
}

/// A build we need to perform to get artifacts to distribute.
enum BuildTarget {
    /// A cargo build
    Cargo(CargoBuildTarget),
    // Other build systems..?
}

/// A cargo build
struct CargoBuildTarget {
    /// The --target triple to pass
    target_triple: String,
    /// The feature flags to pass
    features: CargoTargetFeatures,
    /// What package to build (or "the workspace")
    package: CargoTargetPackages,
    /// The --profile to pass
    profile: String,
    /// Artifacts we expect from this build
    expected_artifacts: Vec<BuildArtifactIdx>,
}

/// An artifact we need from our builds
enum BuildArtifact {
    /// An executable
    Executable(ExecutableBuildArtifact),
}

/// An executable we need from our builds
struct ExecutableBuildArtifact {
    /// The name of the executable (without a file extension)
    exe_name: String,
    /// The cargo package this executable is defined by
    package_id: PackageId,
    /// The [`BuildTarget`][] that should produce this.
    build_target: BuildTargetIdx,
}

/// A distributable bundle we want to build
struct DistributableTarget {
    /// The target platform
    ///
    /// i.e. `x86_64-pc-windows-msvc`
    target_triple: String,
    /// The full name of the distributable
    ///
    /// i.e. `cargo-dist-v0.1.0-x86_64-pc-windows-msvc`
    full_name: String,
    /// The path to the directory where this distributable's
    /// contents will be gathered before bundling.
    ///
    /// i.e. `/.../target/dist/cargo-dist-v0.1.0-x86_64-pc-windows-msvc/`
    dir_path: Utf8PathBuf,
    /// The file name of the distributable
    ///
    /// i.e. `cargo-dist-v0.1.0-x86_64-pc-windows-msvc.zip`
    file_name: String,
    /// The path where the final distributable will appear
    ///
    /// i.e. `/.../target/dist/cargo-dist-v0.1.0-x86_64-pc-windows-msvc.zip`
    file_path: Utf8PathBuf,
    /// The bundling method (zip, tar.gz, ...)
    bundle: BundleStyle,
    /// The build artifacts this distributable will contain
    ///
    /// i.e. `cargo-dist.exe`
    required_artifacts: HashSet<BuildArtifactIdx>,
    /// Additional static assets to add to the distributable
    ///
    /// i.e. `README.md`
    assets: Vec<Utf8PathBuf>,
}

/// A logical release of an application that distributables are grouped under
struct ReleaseTarget {
    /// The name of the app
    app_name: String,
    /// The version of the app
    version: Version,
    /// The distributables this release includes
    distributables: Vec<DistributableTargetIdx>,
}

/// The style of bundle for a [`DistributableTarget`][].
enum BundleStyle {
    /// `.zip`
    Zip,
    /// `.tar.<compression>`
    Tar(CompressionImpl),
    // TODO: Microsoft MSI installer
    // TODO: Apple .dmg "installer"
    // TODO: flatpak?
    // TODO: snap? (ostensibly "obsoleted" by flatpak)
    // TODO: various linux package manager manifests? (.deb, .rpm, ... do these make sense?)
}

/// Compression impls (used by [`BundleStyle::Tar`][])
enum CompressionImpl {
    /// `.gz`
    Gzip,
    /// `.xz`
    Xzip,
    /// `.zstd`
    Zstd,
}

/// Cargo features a [`CargoBuildTarget`][] should use.
struct CargoTargetFeatures {
    /// Whether to disable default features
    no_default_features: bool,
    /// Features to enable
    features: CargoTargetFeatureList,
}

/// A list of features to build with
enum CargoTargetFeatureList {
    /// All of them
    All,
    /// Some of them
    List(Vec<String>),
}

/// Whether to build a package or workspace
enum CargoTargetPackages {
    /// Build the workspace
    Workspace,
    /// Just build a package
    Package(PackageId),
}

/// Top level command of cargo_dist -- do everything!
pub fn do_dist() -> Result<DistReport> {
    let dist = gather_work()?;

    // TODO: parallelize this by working this like a dependency graph, so we can start
    // bundling up an executable the moment it's built!

    // First set up our target dirs so things don't have to race to do it later
    if !dist.dist_dir.exists() {
        std::fs::create_dir_all(&dist.dist_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("couldn't create dist target dir at {}", dist.dist_dir))?;
    }

    for distrib in &dist.distributables {
        eprintln!("bundling {}", distrib.file_name);
        init_distributable_dir(&dist, distrib)?;
    }

    let mut built_artifacts = HashMap::new();
    // Run all the builds
    for target in &dist.targets {
        let new_built_artifacts = build_target(&dist, target)?;
        // Copy the artifacts as soon as possible, future builds may clobber them!
        for (&artifact_idx, built_artifact) in &new_built_artifacts {
            populate_distributable_dirs_with_built_artifact(&dist, artifact_idx, built_artifact)?;
        }
        built_artifacts.extend(new_built_artifacts);
    }

    // Build all the bundles
    for distrib in &dist.distributables {
        populate_distributable_dir_with_assets(&dist, distrib)?;
        bundle_distributable(&dist, distrib)?;
        eprintln!("bundled {}", distrib.file_path);
    }

    // Report the releases
    let mut releases = vec![];
    for release in &dist.releases {
        releases.push(Release {
            app_name: release.app_name.clone(),
            app_version: release.version.to_string(),
            distributables: release
                .distributables
                .iter()
                .map(|distrib_idx| {
                    let distrib = &dist.distributables[distrib_idx.0];
                    Distributable {
                        path: distrib.file_path.clone().into_std_path_buf(),
                        target_triple: distrib.target_triple.clone(),
                        artifacts: distrib
                            .required_artifacts
                            .iter()
                            .map(|artifact_idx| {
                                let artifact = &dist.artifacts[artifact_idx.0];
                                let artifact_path = &built_artifacts[artifact_idx];
                                match artifact {
                                    BuildArtifact::Executable(exe) => {
                                        Artifact::Executable(ExecutableArtifact {
                                            name: exe.exe_name.clone(),
                                            path: PathBuf::from(artifact_path.file_name().unwrap()),
                                        })
                                    }
                                }
                            })
                            .collect(),
                        kind: cargo_dist_schema::DistributableKind::Zip,
                    }
                })
                .collect(),
        })
    }
    Ok(DistReport::new(releases))
}

/// Precompute all the work this invocation will need to do
fn gather_work() -> Result<DistGraph> {
    let cargo = cargo()?;
    let pkg_graph = package_graph(&cargo)?;
    let workspace = workspace_info(&pkg_graph)?;

    // TODO: use this (currently empty)
    let _workspace_config = pkg_graph
        .workspace()
        .metadata_table()
        .get(METADATA_DIST)
        .map(DistMetadata::deserialize)
        .transpose()
        .into_diagnostic()
        .wrap_err("couldn't parse [workspace.metadata.dist]")?;

    // Currently just assume we're in a workspace, no single package!
    /*
    let root_package = binaries.get(0).map(|(p, _)| p).unwrap();
    let local_config = binaries
        .get(0)
        .and_then(|(p, _)| p.metadata_table().get(METADATA_DIST))
        .map(DistMetadata::deserialize)
        .transpose()
        .into_diagnostic()
        .wrap_err("couldn't parse package's [metadata.dist]")?;
     */

    let target_dir = workspace.info.target_directory().to_owned();
    let workspace_dir = workspace.info.root().to_owned();
    let dist_dir = target_dir.join(TARGET_DIST);

    // Currently just build the host target
    let host_target_triple = get_host_target(&cargo)?;
    let mut targets = vec![BuildTarget::Cargo(CargoBuildTarget {
        // Just use the host target for now
        target_triple: host_target_triple,
        // Just build the whole workspace for now
        package: CargoTargetPackages::Workspace,
        // Just use the default build for now
        features: CargoTargetFeatures {
            no_default_features: false,
            features: CargoTargetFeatureList::List(vec![]),
        },
        // Release is the GOAT profile, *obviously*
        profile: String::from(PROFILE_DIST),
        // Populated later
        expected_artifacts: vec![],
    })];

    // Find all the binaries that each target will build
    let mut artifacts = vec![];
    for (idx, target) in targets.iter_mut().enumerate() {
        let target_idx = BuildTargetIdx(idx);
        match target {
            BuildTarget::Cargo(target) => {
                let new_artifacts = match &target.package {
                    CargoTargetPackages::Workspace => artifacts_for_cargo_packages(
                        target_idx,
                        workspace.members.packages(DependencyDirection::Forward),
                    ),
                    CargoTargetPackages::Package(package_id) => {
                        artifacts_for_cargo_packages(target_idx, pkg_graph.metadata(package_id))
                    }
                };
                let new_artifact_idxs = artifacts.len()..artifacts.len() + new_artifacts.len();
                artifacts.extend(new_artifacts);
                target
                    .expected_artifacts
                    .extend(new_artifact_idxs.map(BuildArtifactIdx));
            }
        }
    }

    // Give each artifact its own distributable (for now)
    let mut distributables = vec![];
    let mut releases = HashMap::<(String, Version), ReleaseTarget>::new();
    for (idx, artifact) in artifacts.iter().enumerate() {
        let artifact_idx = BuildArtifactIdx(idx);
        match artifact {
            BuildArtifact::Executable(exe) => {
                let build_target = &targets[exe.build_target.0];
                let target_triple = match build_target {
                    BuildTarget::Cargo(target) => target.target_triple.clone(),
                };

                // TODO: make bundle style configurable
                let target_is_windows = target_triple.contains("windows");
                let bundle = if target_is_windows {
                    // Windows loves them zips
                    BundleStyle::Zip
                } else {
                    // tar.xz is well-supported everywhere and much better than tar.gz
                    BundleStyle::Tar(CompressionImpl::Xzip)
                };

                // TODO: make bundled assets configurable
                // TODO: narrow this scope to the package of the binary..?
                let assets = BUILTIN_FILES
                    .iter()
                    .filter_map(|f| {
                        let file = workspace_dir.join(f);
                        file.exists().then_some(file)
                    })
                    .collect();

                // TODO: make app name configurable? Use some other fields in the PackageMetadata?
                let app_name = exe.exe_name.clone();
                // TODO: allow apps to be versioned separately from packages?
                let version = pkg_graph
                    .metadata(&exe.package_id)
                    .unwrap()
                    .version()
                    .clone();
                // TODO: make the bundle name configurable?
                let full_name = format!("{app_name}-v{version}-{target_triple}");
                let dir_path = dist_dir.join(&full_name);
                let file_ext = match bundle {
                    BundleStyle::Zip => "zip",
                    BundleStyle::Tar(CompressionImpl::Gzip) => "tar.gz",
                    BundleStyle::Tar(CompressionImpl::Zstd) => "tar.zstd",
                    BundleStyle::Tar(CompressionImpl::Xzip) => "tar.xz",
                };
                let file_name = format!("{full_name}.{file_ext}");
                let file_path = dist_dir.join(&file_name);

                let distributable_idx = DistributableTargetIdx(distributables.len());
                distributables.push(DistributableTarget {
                    target_triple,
                    full_name,
                    file_path,
                    file_name,
                    dir_path,
                    bundle,
                    required_artifacts: Some(artifact_idx).into_iter().collect(),
                    assets,
                });
                let release = releases
                    .entry((app_name.clone(), version.clone()))
                    .or_insert_with(|| ReleaseTarget {
                        app_name,
                        version,
                        distributables: vec![],
                    });
                release.distributables.push(distributable_idx);
            }
        }
    }

    let releases = releases.into_iter().map(|e| e.1).collect();
    Ok(DistGraph {
        cargo,
        target_dir,
        workspace_dir,
        dist_dir,
        targets,
        artifacts,
        distributables,
        releases,
    })
}

/// Get all the artifacts built by this list of cargo packages
fn artifacts_for_cargo_packages<'a>(
    target_idx: BuildTargetIdx,
    packages: impl IntoIterator<Item = PackageMetadata<'a>>,
) -> Vec<BuildArtifact> {
    packages
        .into_iter()
        .flat_map(|package| {
            package.build_targets().filter_map(move |target| {
                let build_id = target.id();
                if let BuildTargetId::Binary(name) = build_id {
                    Some(BuildArtifact::Executable(ExecutableBuildArtifact {
                        exe_name: name.to_owned(),
                        package_id: package.id().clone(),
                        build_target: target_idx,
                    }))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>()
}

/// Get the host target triple from cargo
fn get_host_target(cargo: &str) -> Result<String> {
    let mut command = Command::new(cargo);
    command.arg("-vV");
    info!("exec: {:?}", command);
    let output = command
        .output()
        .into_diagnostic()
        .wrap_err("failed to run 'cargo -vV' (trying to get info about host platform)")?;
    let output = String::from_utf8(output.stdout)
        .into_diagnostic()
        .wrap_err("'cargo -vV' wasn't utf8? Really?")?;
    for line in output.lines() {
        if let Some(target) = line.strip_prefix("host: ") {
            info!("host target is {target}");
            return Ok(target.to_owned());
        }
    }
    Err(miette!(
        "'cargo -vV' failed to report its host target? Really?"
    ))
}

/// Build a target
fn build_target(
    dist_graph: &DistGraph,
    target: &BuildTarget,
) -> Result<HashMap<BuildArtifactIdx, Utf8PathBuf>> {
    match target {
        BuildTarget::Cargo(target) => build_cargo_target(dist_graph, target),
    }
}

/// Build a cargo target
fn build_cargo_target(
    dist_graph: &DistGraph,
    target: &CargoBuildTarget,
) -> Result<HashMap<BuildArtifactIdx, Utf8PathBuf>> {
    eprintln!(
        "building cargo target ({}/{})",
        target.target_triple, target.profile
    );
    // Run the build

    // TODO: figure out a principled way for us to add things to RUSTFLAGS
    // without breaking everything. Cargo has some builtin ways like keys
    // in [target...] tables that will get "merged" with the flags it wants
    // to set. More blunt approaches like actually setting the environment
    // variable I think can result in overwriting flags other places set
    // (which is defensible, having spaghetti flags randomly injected by
    // a dozen different tools is a build maintenance nightmare!)

    // TODO: on windows, set RUSTFLAGS="-Ctarget-feature=+crt-static"
    // See: https://rust-lang.github.io/rfcs/1721-crt-static.html
    //
    // Essentially you're *supposed* to be statically linking the windows """libc"""
    // because it's actually a wrapper around more fundamental DLLs and not
    // actually guaranteed to be on the system. This is why lots of games
    // install a C/C++ runtime in their wizards! Unclear what the cost/benefit
    // is of "install" vs "statically link", especially if you only need C
    // and not all of C++. I am however unclear on "which" "libc" you're statically
    // linking. More Research Needed.
    //
    // For similar reasons we may want to perfer targetting "linux-musl" over
    // "linux-gnu" -- the former statically links libc and makes us more portable
    // to "weird" linux setups like NixOS which apparently doesn't have like
    // /etc or /lib to try to try to force things to properly specify their deps
    // (statically linking libc says "no deps pls" (except for specific linux syscalls probably)).
    // I am however vaguely aware of issues where some system magic is hidden away
    // in the gnu libc (glibc) and musl subsequently diverges and acts wonky?
    // This is all vague folklore to me, so More Research Needed.
    //
    // Just to round things out, let's discuss macos. I've never heard of these kinds
    // of issues wrt macos! However I am vaguely aware that macos has an "sdk version"
    // system, which vaguely specifies what APIs you're allowing yourself to use so
    // you can be compatible with any system at least that new (so the older the SDK,
    // the more compatible you are). Do we need to care about that? More Research Needed.

    // TODO: maybe set RUSTFLAGS="-Cforce-frame-pointers=yes"
    //
    // On linux and macos this can make the unwind tables (debuginfo) smaller, more reliable,
    // and faster at minimal runtime cost (these days). This can be a big win for profilers
    // and crash reporters, which both want to unwind in "weird" places quickly and reliably.
    //
    // On windows this setting is unfortunately useless because Microsoft specified
    // it to be... Wrong. Specifically it points "somewhere" in the frame (instead of
    // at the start), and exists only to enable things like -Oz.
    // See: https://github.com/rust-lang/rust/issues/82333

    // TODO: maybe set RUSTFLAGS="-Csymbol-mangling-version=v0"
    // See: https://github.com/rust-lang/rust/issues/60705
    //
    // Despite the name, v0 is actually the *second* mangling format for Rust symbols.
    // The first was more unprincipled and adhoc, and is just the unnamed current
    // default. In the future v0 should become the default. Currently we're waiting
    // for as many tools as possible to add support (and then make it onto dev machines).
    //
    // The v0 scheme is bigger and contains more rich information (with its own fancy
    // compression scheme to try to compensate). Unclear on the exact pros/cons of
    // opting into it earlier.

    // TODO: is there *any* world where we can help the user use Profile Guided Optimization (PGO)?
    // See: https://doc.rust-lang.org/rustc/profile-guided-optimization.html
    // See: https://blog.rust-lang.org/inside-rust/2020/11/11/exploring-pgo-for-the-rust-compiler.html
    //
    // In essence PGO is a ~three-step process:
    //
    // 1. Build your program
    // 2. Run it on a "representative" workload and record traces of the execution ("a profile")
    // 3. Rebuild your program with the profile to Guide Optimization
    //
    // For instance the compiler might see that a certain branch (if) always goes one way
    // in the profile, and optimize the code to go faster if that holds true (by say outlining
    // the other path).
    //
    // PGO can get *huge* wins but is at the mercy of step 2, which is difficult/impossible
    // for a tool like cargo-dist to provide "automatically". But maybe we can streamline
    // some of the rough edges? This is also possibly a place where A Better Telemetry Solution
    // could do some interesting things for dev-controlled production environments.

    // TODO: can we productively use RUSTFLAGS="--remap-path-prefix"?
    // See: https://doc.rust-lang.org/rustc/command-line-arguments.html#--remap-path-prefix-remap-source-names-in-output
    // See: https://github.com/rust-lang/rust/issues/87805
    //
    // Compiler toolchains like stuffing absolute host system paths in metadata/debuginfo,
    // which can make things Bigger and also leak a modicum of private info. This flag
    // lets you specify a rewrite rule for a prefix of the path, letting you map e.g.
    // "C:\Users\Aria\checkouts\cargo-dist\src\main.rs" to ".\cargo-dist\src\main.rs".
    //
    // Unfortunately this is a VERY blunt instrument which does legit exact string matching
    // and can miss paths in places rustc doesn't Expect/See. Still it might be worth
    // setting it in case it Helps?

    let mut command = Command::new(&dist_graph.cargo);
    command
        .arg("build")
        .arg("--profile")
        .arg(&target.profile)
        .arg("--message-format=json")
        .stdout(std::process::Stdio::piped());
    if target.features.no_default_features {
        command.arg("--no-default-features");
    }
    match &target.features.features {
        CargoTargetFeatureList::All => {
            command.arg("--all-features");
        }
        CargoTargetFeatureList::List(features) => {
            if !features.is_empty() {
                command.arg("--features");
                for feature in features {
                    command.arg(feature);
                }
            }
        }
    }
    match &target.package {
        CargoTargetPackages::Workspace => {
            command.arg("--workspace");
        }
        CargoTargetPackages::Package(package) => {
            command.arg("--package").arg(package.to_string());
        }
    }
    info!("exec: {:?}", command);
    let mut task = command
        .spawn()
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to exec cargo build: {:?}", command))?;

    // Create entries for all the binaries we expect to find with empty paths
    // we'll fail if any are still empty at the end!
    let mut expected_exes =
        HashMap::<String, HashMap<String, (BuildArtifactIdx, Utf8PathBuf)>>::new();
    for artifact_idx in &target.expected_artifacts {
        let artifact = &dist_graph.artifacts[artifact_idx.0];
        let BuildArtifact::Executable(exe) = artifact;
        {
            let package_id = exe.package_id.to_string();
            let exe_name = exe.exe_name.clone();
            expected_exes
                .entry(package_id)
                .or_default()
                .insert(exe_name, (*artifact_idx, Utf8PathBuf::new()));
        }
    }

    // Collect up the compiler messages to find out where binaries ended up
    let reader = std::io::BufReader::new(task.stdout.take().unwrap());
    for message in cargo_metadata::Message::parse_stream(reader) {
        let Ok(message) = message.into_diagnostic().wrap_err("failed to parse cargo json message").map_err(|e| warn!("{:?}", e)) else {
            // It's ok for there to be messages we don't understand if we don't care about them.
            // At the end we'll check if we got the messages we *do* need.
            continue;
        };
        match message {
            cargo_metadata::Message::CompilerArtifact(artifact) => {
                // Hey we got an executable, is it one we wanted?
                if let Some(new_exe) = artifact.executable {
                    info!("got a new exe: {}", new_exe);
                    let package_id = artifact.package_id.to_string();
                    let exe_name = new_exe.file_stem().unwrap();
                    let expected_exe = expected_exes
                        .get_mut(&package_id)
                        .and_then(|m| m.get_mut(exe_name));
                    if let Some(expected) = expected_exe {
                        // It is! Save the path.
                        expected.1 = new_exe;
                    }
                }
            }
            _ => {
                // Nothing else interesting?
            }
        }
    }

    // Check that we got everything we expected, and normalize to ArtifactIdx => Artifact Path
    let mut built_exes = HashMap::new();
    for (package_id, exes) in expected_exes {
        for (exe, (artifact_idx, exe_path)) in exes {
            if exe_path.as_str().is_empty() {
                return Err(miette!("failed to find bin {} for {}", exe, package_id));
            }
            built_exes.insert(artifact_idx, exe_path);
        }
    }

    Ok(built_exes)
}

/// Initialize the dir for a distributable (and delete the old distributable file).
fn init_distributable_dir(_dist: &DistGraph, distrib: &DistributableTarget) -> Result<()> {
    info!("recreating distributable dir: {}", distrib.dir_path);

    // Clear out the dir we'll build the bundle up in
    if distrib.dir_path.exists() {
        std::fs::remove_dir_all(&distrib.dir_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to delete old distributable dir {}",
                    distrib.dir_path
                )
            })?;
    }
    std::fs::create_dir(&distrib.dir_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to create distributable dir {}", distrib.dir_path))?;

    // Delete any existing bundle
    if distrib.file_path.exists() {
        std::fs::remove_file(&distrib.file_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!("failed to delete old distributable {}", distrib.file_path)
            })?;
    }

    Ok(())
}

fn populate_distributable_dirs_with_built_artifact(
    dist: &DistGraph,
    artifact_idx: BuildArtifactIdx,
    built_artifact: &Utf8Path,
) -> Result<()> {
    for distrib in &dist.distributables {
        if distrib.required_artifacts.contains(&artifact_idx) {
            let artifact_file_name = built_artifact.file_name().unwrap();
            let bundled_artifact = distrib.dir_path.join(artifact_file_name);
            info!("  adding {built_artifact} to {}", distrib.dir_path);
            std::fs::copy(built_artifact, &bundled_artifact)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!(
                        "failed to copy bundled artifact to distributable: {} => {}",
                        built_artifact, bundled_artifact
                    )
                })?;
        }
    }
    Ok(())
}

fn populate_distributable_dir_with_assets(
    _dist: &DistGraph,
    distrib: &DistributableTarget,
) -> Result<()> {
    info!("populating distributable dir: {}", distrib.dir_path);
    // Copy assets
    for asset in &distrib.assets {
        let asset_file_name = asset.file_name().unwrap();
        let bundled_asset = distrib.dir_path.join(asset_file_name);
        info!("  adding {bundled_asset}");
        std::fs::copy(asset, &bundled_asset)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to copy bundled asset to distributable: {} => {}",
                    asset, bundled_asset
                )
            })?;
    }

    Ok(())
}

fn bundle_distributable(dist_graph: &DistGraph, distrib: &DistributableTarget) -> Result<()> {
    info!("bundling distributable: {}", distrib.file_path);
    match &distrib.bundle {
        BundleStyle::Zip => zip_distributable(dist_graph, distrib),
        BundleStyle::Tar(compression) => tar_distributable(dist_graph, distrib, compression),
    }
}

fn tar_distributable(
    _dist_graph: &DistGraph,
    distrib: &DistributableTarget,
    compression: &CompressionImpl,
) -> Result<()> {
    // Set up the archive/compression
    // The contents of the zip (e.g. a tar)
    let distrib_dir_name = &distrib.full_name;
    let zip_contents_name = format!("{distrib_dir_name}.tar");
    let final_zip_path = &distrib.file_path;
    let final_zip_file = File::create(final_zip_path)
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to create file for distributable: {}",
                final_zip_path
            )
        })?;

    match compression {
        CompressionImpl::Gzip => {
            // Wrap our file in compression
            let zip_output = GzBuilder::new()
                .filename(zip_contents_name)
                .write(final_zip_file, Compression::default());

            // Write the tar to the compression stream
            let mut tar = tar::Builder::new(zip_output);

            // Add the whole dir to the tar
            tar.append_dir_all(distrib_dir_name, &distrib.dir_path)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!(
                        "failed to copy directory into tar: {} => {}",
                        distrib.dir_path, distrib_dir_name
                    )
                })?;
            // Finish up the tarring
            let zip_output = tar
                .into_inner()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write tar: {}", final_zip_path))?;
            // Finish up the compression
            let _zip_file = zip_output
                .finish()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write archive: {}", final_zip_path))?;
            // Drop the file to close it
        }
        CompressionImpl::Xzip => {
            let zip_output = XzEncoder::new(final_zip_file, 9);
            // Write the tar to the compression stream
            let mut tar = tar::Builder::new(zip_output);

            // Add the whole dir to the tar
            tar.append_dir_all(distrib_dir_name, &distrib.dir_path)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!(
                        "failed to copy directory into tar: {} => {}",
                        distrib.dir_path, distrib_dir_name
                    )
                })?;
            // Finish up the tarring
            let zip_output = tar
                .into_inner()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write tar: {}", final_zip_path))?;
            // Finish up the compression
            let _zip_file = zip_output
                .finish()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write archive: {}", final_zip_path))?;
            // Drop the file to close it
        }
        CompressionImpl::Zstd => {
            // Wrap our file in compression
            let zip_output = ZlibEncoder::new(final_zip_file, Compression::default());

            // Write the tar to the compression stream
            let mut tar = tar::Builder::new(zip_output);

            // Add the whole dir to the tar
            tar.append_dir_all(distrib_dir_name, &distrib.dir_path)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!(
                        "failed to copy directory into tar: {} => {}",
                        distrib.dir_path, distrib_dir_name
                    )
                })?;
            // Finish up the tarring
            let zip_output = tar
                .into_inner()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write tar: {}", final_zip_path))?;
            // Finish up the compression
            let _zip_file = zip_output
                .finish()
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write archive: {}", final_zip_path))?;
            // Drop the file to close it
        }
    }

    info!("distributable created at: {}", final_zip_path);
    Ok(())
}

fn zip_distributable(_dist_graph: &DistGraph, distrib: &DistributableTarget) -> Result<()> {
    // Set up the archive/compression
    let final_zip_path = &distrib.file_path;
    let final_zip_file = File::create(final_zip_path)
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to create file for distributable: {}",
                final_zip_path
            )
        })?;

    // Wrap our file in compression
    let mut zip = ZipWriter::new(final_zip_file);

    let dir = std::fs::read_dir(&distrib.dir_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read distributable dir: {}", distrib.dir_path))?;
    for entry in dir {
        let entry = entry.into_diagnostic()?;
        if entry.file_type().into_diagnostic()?.is_file() {
            let options = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            let file = File::open(entry.path()).into_diagnostic()?;
            let mut buf = BufReader::new(file);
            let file_name = entry.file_name();
            // TODO: ...don't do this lossy conversion?
            let utf8_file_name = file_name.to_string_lossy();
            zip.start_file(utf8_file_name.clone(), options)
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!(
                        "failed to create file {} in zip: {}",
                        utf8_file_name, final_zip_path
                    )
                })?;
            std::io::copy(&mut buf, &mut zip).into_diagnostic()?;
        } else {
            panic!("TODO: implement zip subdirs! (or was this a symlink?)");
        }
    }

    // Finish up the compression
    let _zip_file = zip
        .finish()
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write archive: {}", final_zip_path))?;
    // Drop the file to close it
    info!("distributable created at: {}", final_zip_path);
    Ok(())
}

/// Get the path/command to invoke Cargo
fn cargo() -> Result<String> {
    let cargo = std::env::var("CARGO").expect("cargo didn't pass itself!?");
    Ok(cargo)
}

/// Get the PackageGraph for the current workspace
fn package_graph(cargo: &str) -> Result<PackageGraph> {
    let mut metadata_cmd = MetadataCommand::new();
    // guppy will source from the same place as us, but let's be paranoid and make sure
    // EVERYTHING is DEFINITELY ALWAYS using the same Cargo!
    metadata_cmd.cargo_path(cargo);

    // TODO: add a bunch of CLI flags for this. Ideally we'd use clap_cargo
    // but it wants us to use `flatten` and then we wouldn't be able to mark
    // the flags as global for all subcommands :(
    let pkg_graph = metadata_cmd
        .build_graph()
        .into_diagnostic()
        .wrap_err("failed to read 'cargo metadata'")?;

    Ok(pkg_graph)
}

/// Info on the current workspace
struct WorkspaceInfo<'pkg_graph> {
    /// Most info on the workspace.
    info: Workspace<'pkg_graph>,
    /// The workspace members.
    members: PackageSet<'pkg_graph>,
    /// Path to the Cargo.toml of the workspace (may be a package's Cargo.toml)
    manifest_path: Utf8PathBuf,
    /// If the manifest_path points to a package, this is the one.
    ///
    /// If this is None, the workspace Cargo.toml is a virtual manifest.
    root_package: Option<PackageMetadata<'pkg_graph>>,
}

/// Computes [`WorkspaceInfo`][] for the current workspace.
fn workspace_info(pkg_graph: &PackageGraph) -> Result<WorkspaceInfo> {
    let workspace = pkg_graph.workspace();
    let members = pkg_graph.resolve_workspace();

    let manifest_path = workspace.root().join("Cargo.toml");
    if !manifest_path.exists() {
        return Err(miette!("couldn't find root workspace Cargo.toml"));
    }
    // If this is Some, then the root Cargo.toml is for a specific package and not a virtual (workspace) manifest.
    // This affects things like [workspace.metadata] vs [package.metadata]
    let root_package = members
        .packages(DependencyDirection::Forward)
        .find(|p| p.manifest_path() == manifest_path);

    Ok(WorkspaceInfo {
        info: workspace,
        members,
        manifest_path,
        root_package,
    })
}

/// Run 'cargo dist init'
pub fn do_init() -> Result<DistReport> {
    let cargo = cargo()?;
    let pkg_graph = package_graph(&cargo)?;
    let workspace = workspace_info(&pkg_graph)?;

    // Load in the workspace toml to edit and write back
    let mut workspace_toml = {
        let mut workspace_toml_file = File::open(&workspace.manifest_path)
            .into_diagnostic()
            .wrap_err("couldn't load root workspace Cargo.toml")?;
        let mut workspace_toml_str = String::new();
        workspace_toml_file
            .read_to_string(&mut workspace_toml_str)
            .into_diagnostic()
            .wrap_err("couldn't read root workspace Cargo.toml")?;
        workspace_toml_str
            .parse::<toml_edit::Document>()
            .into_diagnostic()
            .wrap_err("couldn't parse root workspace Cargo.toml")?
    };

    // Setup the [profile.dist]
    {
        let profiles = workspace_toml["profile"].or_insert(toml_edit::table());
        if let Some(t) = profiles.as_table_mut() {
            t.set_implicit(true)
        }
        let dist_profile = &mut profiles[PROFILE_DIST];
        if !dist_profile.is_none() {
            return Err(miette!(
                "already init! (based on [profile.dist] existing in your Cargo.toml)"
            ));
        }
        let mut new_profile = toml_edit::table();
        {
            let new_profile = new_profile.as_table_mut().unwrap();
            // We're building for release, so this is a good base!
            new_profile.insert("inherits", toml_edit::value("release"));
            // We want *full* debuginfo for good crashreporting/profiling
            // This doesn't bloat the final binary because we use split-debuginfo below
            new_profile.insert("debug", toml_edit::value(true));
            // Ensure that all debuginfo is pulled out of the binary and tossed
            // into a separate file from the final binary. This should ideally be
            // uploaded to something like a symbol server to be fetched on demand.
            // This is already the default on windows (.pdb) and macos (.dsym) but
            // is rather bleeding on other platforms (.dwp) -- it requires Rust 1.65,
            // which as of this writing in the latest stable rust release! If anyone
            // ever makes a big deal with building final binaries with an older MSRV
            // we may need to more intelligently select this.
            new_profile.insert("split-debuginfo", toml_edit::value("packed"));

            // TODO: set codegen-units=1? (Probably Not!)
            //
            // Ok so there's an inherent tradeoff in compilers where if the compiler does
            // everything in a very serial/global way, it can discover more places where
            // optimizations can be done and theoretically make things faster/smaller
            // using all the information at its fingertips... at the cost of your builds
            // taking forever. Compiler optimizations generally take super-linear time,
            // so if you let the compiler see and think about EVERYTHING your builds
            // can literally take *days* for codebases on the order of LLVM itself.
            //
            // To keep compile times tractable, we generally break up the program
            // into "codegen units" (AKA "translation units") that get compiled
            // independently and then combined by the linker. This keeps the super-linear
            // scaling under control, but prevents optimizations like inlining across
            // units. (This process is why we have things like "object files" and "rlibs",
            // those are the intermediate artifacts fed to the linker.)
            //
            // Compared to C, Rust codegen units are quite monolithic. Where each C
            // *file* might gets its own codegen unit, Rust prefers scoping them to
            // an entire *crate*.  As it turns out, neither of these answers is right in
            // all cases, and being able to tune the unit size is useful.
            //
            // Large C++ codebases like Firefox have "unified" builds where they basically
            // concatenate files together to get bigger units. Rust provides the
            // opposite: the codegen-units=N option tells rustc that it should try to
            // break up a crate into at most N different units. This is done with some
            // heuristics and contraints to try to still get the most out of each unit
            // (i.e. try to keep functions that call eachother together for inlining).
            //
            // In the --release profile, codegen-units is set to 16, which attempts
            // to strike a balance between The Best Binaries and Ever Finishing Compiles.
            // In principle, tuning this down to 1 could be profitable, but LTO
            // (see the next TODO) does most of that work for us. As such we can probably
            // leave this alone to keep compile times reasonable.

            // TODO: set lto="thin" (or "fat")? (Probably "fat"!)
            //
            // LTO, Link Time Optimization, is basically hijacking the step where we
            // would link together everything and going back to the compiler (LLVM) to
            // do global optimizations across codegen-units (see the previous TODO).
            // Better Binaries, Slower Build Times.
            //
            // LTO can be "fat" (or "full") or "thin".
            //
            // Fat LTO is the "obvious" implementation: once you're done individually
            // optimizing the LLVM bitcode (IR) for each compilation unit, you concatenate
            // all the units and optimize it all together. Extremely serial, extremely
            // slow, but thorough as hell. For *enormous* codebases (millions of lines)
            // this can become intractably expensive and crash the compiler.
            //
            // Thin LTO is newer and more complicated: instead of unconditionally putting
            // everything together, we want to optimize each unit with other "useful" units
            // pulled in for inlining and other analysis. This grouping is done with
            // similar heuristics that rustc uses to break crates into codegen-units.
            // This is much faster than Fat LTO and can scale to arbitrarily big
            // codebases, but does produce slightly worse results.
            //
            // Release builds currently default to lto=false, which, despite the name,
            // actually still does LTO (lto="off" *really* turns it off)! Specifically it
            // does Thin LTO but *only* between the codegen units for a single crate.
            // This theoretically negates the disadvantages of codegen-units=16 while
            // still getting most of the advantages! Neat!
            //
            // Since most users will have codebases significantly smaller than An Entire
            // Browser, we can probably go all the way to default lto="fat", and they
            // can tune that down if it's problematic. If a user has "nightly" and "stable"
            // builds, it might be the case that they want lto="thin" for the nightlies
            // to keep them timely.
            //
            // > Aside: you may be wondering "why still have codegen units at all if using
            // > Fat LTO" and the best answer I can give you is "doing things in parallel
            // > at first lets you throw out a lot of junk and trim down the input before
            // > starting the really expensive super-linear global analysis, without losing
            // > too much of the important information". The less charitable answer is that
            // > compiler infra is built around codegen units and so this is a pragmatic hack.
            // >
            // > Thin LTO of course *really* benefits from still having codegen units.

            // TODO: set panic="abort"?
            //
            // PROBABLY NOT, but here's the discussion anyway!
            //
            // The default is panic="unwind", and things can be relying on unwinding
            // for correctness. Unwinding support bloats up the binary and can make
            // code run slower (because each place that *can* unwind is essentially
            // an early-return the compiler needs to be cautious of).
            //
            // panic="abort" immediately crashes the program if you panic,
            // but does still run the panic handler, so you *can* get things like
            // backtraces/crashreports out at that point.
            //
            // See RUSTFLAGS="-Cforce-unwind-tables" for the semi-orthogonal flag
            // that adjusts whether unwinding tables are emitted at all.
            //
            // Major C++ applications like Firefox already build with this flag,
            // the Rust ecosystem largely works fine with either.

            new_profile
                .decor_mut()
                .set_prefix("\n# generated by 'cargo dist init'\n")
        }
        dist_profile.or_insert(new_profile);
    }
    // Setup [workspace.metadata.dist] or [package.metadata.dist]
    {
        let metadata_pre_key = if workspace.root_package.is_some() {
            "package"
        } else {
            "workspace"
        };
        let workspace = workspace_toml[metadata_pre_key].or_insert(toml_edit::table());
        if let Some(t) = workspace.as_table_mut() {
            t.set_implicit(true)
        }
        let metadata = workspace["metadata"].or_insert(toml_edit::table());
        if let Some(t) = metadata.as_table_mut() {
            t.set_implicit(true)
        }
        let dist_metadata = &mut metadata[METADATA_DIST];
        if !dist_metadata.is_none() {
            return Err(miette!(
                "already init! (based on [workspace.metadata.dist] existing in your Cargo.toml)"
            ));
        }
        let mut new_metadata = toml_edit::table();
        {
            let new_metadata = new_metadata.as_table_mut().unwrap();
            new_metadata.insert(
                "os",
                toml_edit::Item::Value([OS_WINDOWS, OS_MACOS, OS_LINUX].into_iter().collect()),
            );
            new_metadata.insert(
                "cpu",
                toml_edit::Item::Value([CPU_X64, CPU_ARM64].into_iter().collect()),
            );
            new_metadata.decor_mut().set_prefix(
                "\n# These keys are generated by 'cargo dist init' and are fake placeholders\n",
            );
        }

        dist_metadata.or_insert(new_metadata);
    }
    {
        use std::io::Write;
        let mut workspace_toml_file = File::options()
            .write(true)
            .open(&workspace.manifest_path)
            .into_diagnostic()
            .wrap_err("couldn't load root workspace Cargo.toml")?;
        writeln!(&mut workspace_toml_file, "{}", workspace_toml)
            .into_diagnostic()
            .wrap_err("failed to write to Cargo.toml")?;
    }
    Ok(DistReport { releases: vec![] })
}