use cargo_dist_schema::TargetTriple;

use crate::platform::targets::{Abi, Arch, Os, TargetTripleParsed, Vendor};

macro_rules! assert_target {
    ($triple:literal => $arch:expr, $vendor:expr, $os:expr, $abi:expr) => {
        let pt = TargetTripleParsed::from(TargetTriple::new($triple.to_string()));
        assert_eq!(pt.original.as_str(), $triple);
        assert_eq!(pt.arch, $arch);
        assert_eq!(pt.vendor, $vendor);
        assert_eq!(pt.os, $os);
        assert_eq!(pt.abi, $abi);
    };
}

#[test]
#[rustfmt::skip]
fn test_target_triple_parsing() {
    // These are based on the contents
    // of <https://doc.rust-lang.org/nightly/rustc/platform-support.html>
    // as of 2024-10-24. They might need to be updated over time,
    // and the rust project might change their minimum glibc requirements etc.

    //----------------------
    // Tier 1 with host tools
    //----------------------

    // ARM64 Linux (kernel 4.1, glibc 2.17+)
    assert_target!("aarch64-unknown-linux-gnu" => Arch::Aarch64, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // ARM64 macOS (11.0+, Big Sur+)
    assert_target!("aarch64-apple-darwin" => Arch::Aarch64, Vendor::Apple, Os::Darwin, Abi::Unknown);
    // 32-bit MinGW (Windows 7+)
    assert_target!("i686-pc-windows-gnu" => Arch::I686, Vendor::Pc, Os::Windows, Abi::Gnu);
    // 32-bit Windows MSVC (Windows 7+)
    assert_target!("i686-pc-windows-msvc" => Arch::I686, Vendor::Pc, Os::Windows, Abi::Msvc);
    // 32-bit Linux (kernel 3.2+, glibc 2.17+)
    assert_target!("i686-unknown-linux-gnu" => Arch::I686, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // 64-bit Intel macOS (10.12+, Sierra+)
    assert_target!("x86_64-apple-darwin" => Arch::X86_64, Vendor::Apple, Os::Darwin, Abi::Unknown);
    // 64-bit MinGW (Windows 7+)
    assert_target!("x86_64-pc-windows-gnu" => Arch::X86_64, Vendor::Pc, Os::Windows, Abi::Gnu);
    // 64-bit Windows MSVC (Windows 7+)
    assert_target!("x86_64-pc-windows-msvc" => Arch::X86_64, Vendor::Pc, Os::Windows, Abi::Msvc);
    // 64-bit Linux (kernel 3.2+, glibc 2.17+)
    assert_target!("x86_64-unknown-linux-gnu" => Arch::X86_64, Vendor::Unknown, Os::Linux, Abi::Gnu);

    //----------------------
    // Tier 2 with host tools
    //----------------------

    // ARM64 Windows MSVC
    assert_target!("aarch64-pc-windows-msvc" => Arch::Aarch64, Vendor::Pc, Os::Windows, Abi::Msvc);
    // ARM64 Linux with musl 1.2.3
    assert_target!("aarch64-unknown-linux-musl" => Arch::Aarch64, Vendor::Unknown, Os::Linux, Abi::Musl);
    // ARMv6 Linux (kernel 3.2, glibc 2.17)
    assert_target!("arm-unknown-linux-gnueabi" => Arch::Arm, Vendor::Unknown, Os::Linux, Abi::Gnueabi);
    // ARMv6 Linux, hardfloat (kernel 3.2, glibc 2.17)
    assert_target!("arm-unknown-linux-gnueabihf" => Arch::Arm, Vendor::Unknown, Os::Linux, Abi::Gnueabihf);
    // ARMv7-A Linux, hardfloat (kernel 3.2, glibc 2.17)
    assert_target!("armv7-unknown-linux-gnueabihf" => Arch::Armv7, Vendor::Unknown, Os::Linux, Abi::Gnueabihf);
    // LoongArch64 Linux, LP64D ABI (kernel 5.19, glibc 2.36)
    assert_target!("loongarch64-unknown-linux-gnu" => Arch::Loongarch64, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // LoongArch64 Linux with MUSL, LP64D ABI
    assert_target!("loongarch64-unknown-linux-musl" => Arch::Loongarch64, Vendor::Unknown, Os::Linux, Abi::Musl);
    // PowerPC Linux (kernel 3.2, glibc 2.17)
    assert_target!("powerpc-unknown-linux-gnu" => Arch::Powerpc, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // PPC64 Linux (kernel 3.2, glibc 2.17)
    assert_target!("powerpc64-unknown-linux-gnu" => Arch::Powerpc64, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // PPC64LE Linux (kernel 3.10, glibc 2.17)
    assert_target!("powerpc64le-unknown-linux-gnu" => Arch::Powerpc64le, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // RISC-V Linux (kernel 4.20, glibc 2.29)
    assert_target!("riscv64gc-unknown-linux-gnu" => Arch::Riscv64gc, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // RISC-V Linux with MUSL
    assert_target!("riscv64gc-unknown-linux-musl" => Arch::Riscv64gc, Vendor::Unknown, Os::Linux, Abi::Musl);
    // S390x Linux (kernel 3.2, glibc 2.17)
    assert_target!("s390x-unknown-linux-gnu" => Arch::S390x, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // 64-bit FreeBSD (version 13.2)
    assert_target!("x86_64-unknown-freebsd" => Arch::X86_64, Vendor::Unknown, Os::Freebsd, Abi::Unknown);
    // illumos
    assert_target!("x86_64-unknown-illumos" => Arch::X86_64, Vendor::Unknown, Os::Illumos, Abi::Unknown);
    // 64-bit Linux with musl 1.2.3
    assert_target!("x86_64-unknown-linux-musl" => Arch::X86_64, Vendor::Unknown, Os::Linux, Abi::Musl);
    // NetBSD/amd64
    assert_target!("x86_64-unknown-netbsd" => Arch::X86_64, Vendor::Unknown, Os::Netbsd, Abi::Unknown);

    //----------------------
    // Tier 2 without host tools
    //----------------------

    // ARM64 iOS
    assert_target!("aarch64-apple-ios" => Arch::Aarch64, Vendor::Apple, Os::Ios, Abi::Unknown);
    // ARM64 Fuchsia
    assert_target!("aarch64-unknown-fuchsia" => Arch::Aarch64, Vendor::Unknown, Os::Fuchsia, Abi::Unknown);
    // ARM64 Android
    assert_target!("aarch64-linux-android" => Arch::Aarch64, Vendor::Unknown, Os::Linux, Abi::Android);
    // 32-bit Linux w/o SSE (kernel 3.2, glibc 2.17)
    assert_target!("i586-unknown-linux-gnu" => Arch::I586, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // 32-bit Linux w/o SSE, musl 1.2.3
    assert_target!("i586-unknown-linux-musl" => Arch::I586, Vendor::Unknown, Os::Linux, Abi::Musl);
    // 32-bit x86 Android
    assert_target!("i686-linux-android" => Arch::I686, Vendor::Unknown, Os::Linux, Abi::Android);
    // 32-bit Linux with musl 1.2.3
    assert_target!("i686-unknown-linux-musl" => Arch::I686, Vendor::Unknown, Os::Linux, Abi::Musl);
    // 32-bit FreeBSD
    assert_target!("i686-unknown-freebsd" => Arch::I686, Vendor::Unknown, Os::Freebsd, Abi::Unknown);
    // SPARC Linux (kernel 4.4, glibc 2.23)
    assert_target!("sparc64-unknown-linux-gnu" => Arch::Sparc64, Vendor::Unknown, Os::Linux, Abi::Gnu);
    // SPARC Solaris 10/11, illumos
    assert_target!("sparcv9-sun-solaris" => Arch::Sparcv9, Vendor::Sun, Os::Solaris, Abi::Unknown);
    // WebAssembly with WASI
    assert_target!("wasm32-wasi" => Arch::Wasm32, Vendor::Unknown, Os::Wasi, Abi::Unknown);
    // WebAssembly
    assert_target!("wasm32-unknown-unknown" => Arch::Wasm32, Vendor::Unknown, Os::Unknown, Abi::Unknown);
}
