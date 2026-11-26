use std::{env, path::Path, path::PathBuf, process::Command};

fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: could not run {error}"));
    assert!(status.success(), "{what} failed with {status}");
}

const CLANG_ARGS: [&str; 6] = [
    "--target=x86_64-pc-windows-msvc",
    "-ffreestanding",
    "-fno-stack-protector",
    "-fno-asynchronous-unwind-tables",
    "-O2",
    "-c",
];

fn compile(source: &Path, out: &Path) {
    run(
        Command::new("clang")
            .args(CLANG_ARGS)
            .arg(source)
            .arg("-o")
            .arg(out),
        &format!("WINPE compile {}", source.display()),
    );
}

fn link(args: Vec<String>, out: &Path, what: &str) {
    let mut command = Command::new("lld-link");
    command.args(&args).arg(format!("/out:{}", out.display()));
    run(&mut command, what);
}

fn build_winpe_fixture() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("probes");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let stub_obj = out.join("winpe-stub.obj");
    let extra_obj = out.join("winpe-extra.obj");
    let fwd_obj = out.join("winpe-fwd.obj");
    let entry_obj = out.join("winpe-entry.obj");
    compile(&source.join("stub.c"), &stub_obj);
    compile(&source.join("extra.c"), &extra_obj);
    compile(&source.join("fwd.c"), &fwd_obj);
    compile(&source.join("winentry.c"), &entry_obj);

    let dll_flags = |base: &str, export: &str, lib: &Path| {
        vec![
            "/dll".to_owned(),
            "/noentry".to_owned(),
            format!("/base:{base}"),
            format!("/export:{export}"),
            format!("/implib:{}", lib.display()),
        ]
    };
    let stub_lib = out.join("winpe-stub.lib");
    let extra_lib = out.join("winpe-extra.lib");
    let fwd_lib = out.join("winpe-fwd.lib");
    link(
        dll_flags("0x180000000", "Stub", &stub_lib)
            .into_iter()
            .chain([stub_obj.display().to_string()])
            .collect(),
        &out.join("winpe-stub.dll"),
        "WINPE stub link",
    );
    link(
        dll_flags("0x190000000", "Extra", &extra_lib)
            .into_iter()
            .chain([extra_obj.display().to_string()])
            .collect(),
        &out.join("winpe-extra.dll"),
        "WINPE extra link",
    );
    // 纯转发 DLL：导出表指向 winpe-stub.Stub，没有代码与导入。
    link(
        vec![
            "/dll".to_owned(),
            "/noentry".to_owned(),
            "/base:0x1A0000000".to_owned(),
            format!("/def:{}", source.join("fwd.def").display()),
            format!("/implib:{}", fwd_lib.display()),
            fwd_obj.display().to_string(),
        ],
        &out.join("winpe-fwd.dll"),
        "WINPE forwarder link",
    );

    link(
        vec![
            "/entry:entry".to_owned(),
            "/subsystem:console".to_owned(),
            "/nodefaultlib".to_owned(),
            "/dynamicbase".to_owned(),
            entry_obj.display().to_string(),
            stub_lib.display().to_string(),
            extra_lib.display().to_string(),
            fwd_lib.display().to_string(),
        ],
        &out.join("winpe-probe.exe"),
        "WINPE entry link",
    );
}

fn main() {
    println!("cargo:rerun-if-changed=probes");
    if env::var_os("CARGO_FEATURE_WINPE_PROBE").is_some() {
        build_winpe_fixture();
    }
}
