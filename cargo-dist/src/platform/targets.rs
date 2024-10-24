//! Information about various supported platforms

/// Useful (parsed, structured) information about a [`TargetTriple`]
/// These are based on rustc target triples, so for example there's no
/// 32-bit `x86` target, there's `i686`.
pub struct TargetTripleParsed {
    /// The architecture, something like `x86_64` or `aarch64`
    pub arch: Arch,

    /// The vendor, something like `apple` or `pc
    pub vendor: Vendor,

    /// The operating system, somsething like `linux` or `windows`
    pub os: Os,

    /// The ABI, something like `gnu` or `msvc`
    pub abi: Abi,

    /// The original target triple, as it was parsed
    pub original: TargetTriple,
}

impl TargetTripleParsed {
    /// Returns whether this is a 64-bit target
    pub fn is_64bit(&self) -> bool {
        self.arch.bit_width().map(|w| w == 64).unwrap_or(false)
    }

    /// Returns whether this is a 32-bit target
    pub fn is_32bit(&self) -> bool {
        self.arch.bit_width().map(|w| w == 32).unwrap_or(false)
    }

    /// Returns whether this is a Windows target
    pub fn is_windows(&self) -> bool {
        matches!(self.os, Os::Windows)
    }

    /// Returns whether this is a macOS target
    pub fn is_mac(&self) -> bool {
        matches!(self.os, Os::Darwin)
    }

    /// Returns whether this is a Linux target
    pub fn is_linux(&self) -> bool {
        matches!(self.os, Os::Linux)
    }

    /// Returns whether this is a GNU Linux target
    pub fn is_gnu(&self) -> bool {
        matches!(self.abi, Abi::Gnu | Abi::Gnueabi | Abi::Gnueabihf)
    }

    /// Returns whether this is a musl target
    pub fn is_musl(&self) -> bool {
        matches!(self.abi, Abi::Musl | Abi::Musleabi | Abi::Musleabihf)
    }

    /// Returns whether this is a MSVC target
    pub fn is_msvc(&self) -> bool {
        matches!(self.abi, Abi::Msvc)
    }

    /// Returns whether this is an iOS target
    pub fn is_ios(&self) -> bool {
        matches!(self.os, Os::Ios)
    }

    /// Returns whether this is an Android target
    pub fn is_android(&self) -> bool {
        matches!(self.os, Os::Android)
    }

    /// Returns whether this is a WASM target
    pub fn is_wasm(&self) -> bool {
        matches!(self.arch, Arch::Wasm32)
    }

    /// Returns whether this is a BSD-based target
    pub fn is_bsd(&self) -> bool {
        matches!(self.os, Os::Freebsd | Os::Netbsd)
    }

    /// Returns whether this is a big-endian target
    pub fn is_big_endian(&self) -> bool {
        self.arch
            .endianness()
            .map(|e| matches!(e, Endianness::Big))
            .unwrap_or(false)
    }

    /// Returns whether this is a little-endian target
    pub fn is_little_endian(&self) -> bool {
        self.arch
            .endianness()
            .map(|e| matches!(e, Endianness::Little))
            .unwrap_or(false)
    }
}

impl From<TargetTriple> for TargetTripleParsed {
    fn from(value: TargetTriple) -> Self {
        // making the executive decision that triples have 4 components max, if you
        // have more, it's part of the "abi gunk".
        let tokens: Vec<_> = value.as_str().splitn(4, '-').collect();
        match tokens[..] {
            [arch, os] => {
                // a-la `wasm32-wasi` or `aarch64-fuchsia`
                Self {
                    arch: Arch::from_str(arch),
                    vendor: Vendor::Unknown,
                    os: Os::from_str(os),
                    abi: Abi::Unknown,
                    original: value,
                }
            }
            [a, b, c] => {
                // are we looking at `i686-linux-android` for example? if so,
                // we actually have `[arch, os, abi]`
                if b == "linux" {
                    let (arch, os, abi) = (a, b, c);
                    Self {
                        arch: Arch::from_str(arch),
                        vendor: Vendor::Unknown,
                        os: Os::from_str(os),
                        abi: Abi::from_str(abi),
                        original: value,
                    }
                } else {
                    // okay good, we're probably looking at something like
                    // `aarch64-apple-darwin` then
                    let (arch, vendor, os) = (a, b, c);
                    Self {
                        arch: Arch::from_str(arch),
                        vendor: Vendor::from_str(vendor),
                        os: Os::from_str(os),
                        abi: Abi::Unknown,
                        original: value,
                    }
                }
            }
            [a, b, c, d] => {
                // in this case we can be relatively sure we have:
                let (arch, vendor, os, abi) = (a, b, c, d);

                // a-la `x86_64-unknown-linux-gnu`
                Self {
                    arch: Arch::from_str(arch),
                    vendor: Vendor::from_str(vendor),
                    os: Os::from_str(os),
                    abi: Abi::from_str(abi),
                    original: value,
                }
            }
            _ => Self {
                arch: Arch::Unknown,
                vendor: Vendor::Unknown,
                os: Os::Unknown,
                abi: Abi::Unknown,
                original: value,
            },
        }
    }
}

// Various stringish enums

declare_stringish_enum! {
    /// An architecture, something like `x86_64` or `aarch64`
    #[allow(missing_docs)]
    pub enum Arch {
        /// Used for architectures not explicitly listed
        Other(String),

        /// Intel i686 (Pentium Pro, Pentium II and later) 32-bit x86 architecture
        /// See: <https://en.wikipedia.org/wiki/P6_(microarchitecture)>
        I686 = "i686",
        /// AMD64/Intel 64 architecture (x86-64)
        /// See: <https://en.wikipedia.org/wiki/X86-64>
        X86_64 = "x86_64",
        /// 64-bit ARM architecture
        /// See: <https://en.wikipedia.org/wiki/AArch64>
        Aarch64 = "aarch64",

        /// ARMv7-A architecture
        /// See: <https://en.wikipedia.org/wiki/ARM_architecture#32-bit_architecture>
        Armv7 = "armv7",
        /// Generic ARM architecture (typically ARMv6)
        /// See: <https://en.wikipedia.org/wiki/ARM_architecture>
        Arm = "arm",
        /// Intel i586 (Pentium MMX) architecture
        /// See: <https://en.wikipedia.org/wiki/P5_(microarchitecture)>
        I586 = "i586",
        /// 32-bit PowerPC architecture
        /// See: <https://en.wikipedia.org/wiki/PowerPC>
        Powerpc = "powerpc",
        /// 64-bit PowerPC architecture (big endian)
        /// See: <https://en.wikipedia.org/wiki/Ppc64>
        Powerpc64 = "powerpc64",
        /// 64-bit PowerPC architecture (little endian)
        /// See: <https://en.wikipedia.org/wiki/Ppc64le>
        Powerpc64le = "powerpc64le",
        /// IBM System/390x architecture
        /// See: <https://en.wikipedia.org/wiki/IBM_System/390>
        S390x = "s390x",
        /// 64-bit RISC-V architecture with general compute extensions
        /// See: <https://en.wikipedia.org/wiki/RISC-V>
        Riscv64gc = "riscv64gc",
        /// LoongArch 64-bit architecture
        /// See: <https://en.wikipedia.org/wiki/Loongson#LoongArch>
        Loongarch64 = "loongarch64",
        /// 64-bit SPARC architecture
        /// See: <https://en.wikipedia.org/wiki/SPARC>
        Sparc64 = "sparc64",
        /// SPARC Version 9 architecture
        /// See: <https://en.wikipedia.org/wiki/SPARC>
        Sparcv9 = "sparcv9",
        /// 32-bit WebAssembly
        /// See: <https://webassembly.org/>
        Wasm32 = "wasm32",

        /// Represents an unknown architecture
        Unknown = "unknown",
    }
}

/// Endianness: big or little
pub enum Endianness {
    /// little-endian (least significant byte first)
    Little,
    /// big-endian (most significant byte first)
    Big,
}

impl Arch {
    /// Returns the bit-width of the architecture, if known
    pub fn bit_width(&self) -> Option<usize> {
        match self {
            Self::I586 => Some(32),
            Self::I686 => Some(32),
            Self::X86_64 => Some(64),
            Self::Aarch64 => Some(64),
            Self::Armv7 => Some(32),
            Self::Arm => Some(32),
            Self::Powerpc => Some(32),
            Self::Powerpc64 | Self::Powerpc64le => Some(64),
            Self::S390x => Some(64),
            Self::Riscv64gc => Some(64),
            Self::Loongarch64 => Some(64),
            Self::Sparc64 | Self::Sparcv9 => Some(64),
            Self::Wasm32 => Some(32),
            Self::Unknown => None,
            Self::Other(_) => None,
        }
    }

    /// Returns the endianness of the architecture, if known
    pub fn endianness(&self) -> Option<Endianness> {
        match self {
            Self::I586 => Some(Endianness::Little),
            Self::I686 => Some(Endianness::Little),
            Self::X86_64 => Some(Endianness::Little),
            Self::Aarch64 => Some(Endianness::Little),
            Self::Armv7 => Some(Endianness::Little),
            Self::Arm => Some(Endianness::Little),
            Self::Powerpc => Some(Endianness::Big),
            Self::Powerpc64 => Some(Endianness::Big),
            Self::Powerpc64le => Some(Endianness::Little),
            Self::S390x => Some(Endianness::Big),
            Self::Riscv64gc => Some(Endianness::Little),
            Self::Loongarch64 => Some(Endianness::Little),
            Self::Sparc64 => Some(Endianness::Big),
            Self::Sparcv9 => Some(Endianness::Big),
            Self::Wasm32 => Some(Endianness::Little),
            Self::Unknown => None,
            Self::Other(_) => None,
        }
    }
}

declare_stringish_enum! {
    /// A vendor, something like `apple` or `pc`
    #[allow(missing_docs)]
    pub enum Vendor {
        /// Used for vendors not explicitly listed
        Other(String),

        /// Apple Inc. (used for Darwin, iOS targets)
        /// See: <https://www.apple.com/>
        Apple = "apple",
        /// Personal Computer (used for windows targets)
        /// See: <https://en.wikipedia.org/wiki/IBM_PC_compatible>
        Pc = "pc",
        /// Sun Microsystems (now Oracle, used for Solaris targets)
        /// See: <https://en.wikipedia.org/wiki/Sun_Microsystems>
        Sun = "sun",
        /// Represents an unknown vendor
        Unknown = "unknown",
    }
}

declare_stringish_enum! {
    /// An operating system, something like `linux` or `windows`
    #[allow(missing_docs)]
    pub enum Os {
        /// Used for operating systems not explicitly listed
        Other(String),

        /// Linux operating system
        /// See: <https://www.kernel.org/>
        Linux = "linux",
        /// Microsoft Windows operating system
        /// See: <https://www.microsoft.com/windows>
        Windows = "windows",
        /// Apple Darwin operating system (macOS, previously "mac OS X" / "OSX")
        /// See: <https://www.apple.com/macos>
        Darwin = "darwin",

        /// FreeBSD operating system
        /// See: <https://www.freebsd.org/>
        Freebsd = "freebsd",
        /// NetBSD operating system
        /// See: <https://www.netbsd.org/>
        Netbsd = "netbsd",
        /// illumos operating system
        /// See: <https://illumos.org/>
        Illumos = "illumos",
        /// Apple iOS mobile operating system
        /// See: <https://www.apple.com/ios>
        Ios = "ios",
        /// Apple watchOS operating system for Apple Watch
        /// See: <https://www.apple.com/watchos>
        Watchos = "watchos",
        /// Apple tvOS operating system for Apple TV
        /// See: <https://www.apple.com/tvos>
        Tvos = "tvos",
        /// Google Fuchsia operating system
        /// See: <https://fuchsia.dev/>
        Fuchsia = "fuchsia",
        /// Android mobile operating system
        /// See: <https://www.android.com/>
        Android = "android",
        /// WebAssembly System Interface
        /// See: <https://wasi.dev/>
        Wasi = "wasi",
        /// Oracle Solaris operating system
        /// See: <https://www.oracle.com/solaris>
        Solaris = "solaris",
        /// Represents an unknown operating system
        Unknown = "unknown",
    }
}

declare_stringish_enum! {
    /// An ABI, something like `gnu` or `msvc`
    #[allow(missing_docs)]
    pub enum Abi {
        /// Used for ABIs not explicitly listed
        Other(String),

        //------------ GNU
        /// GNU ABI (used for Linux glibc targets)
        /// See: <https://en.wikipedia.org/wiki/GNU>
        Gnu = "gnu",
        /// GNU ABI for embedded ARM targets
        /// See: <https://wiki.debian.org/ArmEabiPort>
        Gnueabi = "gnueabi",
        /// GNU ABI for embedded ARM targets with hardware floating point
        /// See: <https://wiki.debian.org/ArmHardFloatPort>
        Gnueabihf = "gnueabihf",

        //------------ Musl
        /// Musl libc ABI (used for Linux musl targets)
        /// See: <https://musl.libc.org/>
        Musl = "musl",
        /// Musl libc ABI for embedded ARM targets
        /// See: <https://musl.libc.org/>
        Musleabi = "musleabi",
        /// Musl libc ABI for embedded ARM targets with hardware floating point
        /// See: <https://musl.libc.org/>
        Musleabihf = "musleabihf",

        //------------ MSVC
        /// Microsoft Visual C++ ABI (used for Windows targets)
        /// See: <https://en.wikipedia.org/wiki/Microsoft_Visual_C%2B%2B#ABI>
        Msvc = "msvc",

        //------------ Android
        /// Android ABI (used for Android targets)
        /// See: <https://source.android.com/docs/core/build-number#platform-versions>
        Android = "android",

        //------------ Other weird ones

        /// Represents an unknown ABI
        Unknown = "unknown",
    }
}

// Various target triples

use cargo_dist_schema::{declare_stringish_enum, TargetTriple, TargetTripleRef};

macro_rules! define_target_triples {
    ($($(#[$meta:meta])* const $name:ident = $triple:expr;)*) => {
        $(
            $(#[$meta])*
            pub const $name: &TargetTripleRef = TargetTripleRef::from_str($triple);
        )*
    };
}

define_target_triples!(
    /// 32-bit Windows MSVC (Windows 7+)
    const TARGET_X86_WINDOWS = "i686-pc-windows-msvc";
    /// 64-bit Windows MSVC (Windows 7+)
    const TARGET_X64_WINDOWS = "x86_64-pc-windows-msvc";
    /// ARM64 Windows MSVC
    const TARGET_ARM64_WINDOWS = "aarch64-pc-windows-msvc";
    /// 32-bit MinGW (Windows 7+)
    const TARGET_X86_MINGW = "i686-pc-windows-gnu";
    /// 64-bit MinGW (Windows 7+)
    const TARGET_X64_MINGW = "x86_64-pc-windows-gnu";
    /// ARM64 MinGW (Windows 7+)
    const TARGET_ARM64_MINGW = "aarch64-pc-windows-gnu";
);

/// List of all recognized Windows targets
pub const KNOWN_WINDOWS_TARGETS: &[&TargetTripleRef] = &[
    TARGET_X86_WINDOWS,
    TARGET_X64_WINDOWS,
    TARGET_ARM64_WINDOWS,
    TARGET_X86_MINGW,
    TARGET_X64_MINGW,
    TARGET_ARM64_MINGW,
];

define_target_triples!(
    /// 32-bit Intel macOS (10.12+, Sierra+)
    const TARGET_X86_MAC = "i686-apple-darwin";
    /// 64-bit Intel macOS (10.12+, Sierra+)
    const TARGET_X64_MAC = "x86_64-apple-darwin";
    /// ARM64 macOS (11.0+, Big Sur+) -- AKA "Apple Silicon"
    const TARGET_ARM64_MAC = "aarch64-apple-darwin";
);

/// List of all recognized Mac targets
pub const KNOWN_MAC_TARGETS: &[&TargetTripleRef] =
    &[TARGET_X86_MAC, TARGET_X64_MAC, TARGET_ARM64_MAC];

define_target_triples!(
    /// 32-bit Linux (kernel 3.2+, glibc 2.17+)
    const TARGET_X86_LINUX_GNU = "i686-unknown-linux-gnu";
    /// 64-bit Linux (kernel 3.2+, glibc 2.17+)
    const TARGET_X64_LINUX_GNU = "x86_64-unknown-linux-gnu";
    /// ARM64 Linux (kernel 4.1, glibc 2.17+)
    const TARGET_ARM64_LINUX_GNU = "aarch64-unknown-linux-gnu";
    /// ARMv7-A Linux, hardfloat (kernel 3.2, glibc 2.17) -- AKA ARMv7-A Linux
    const TARGET_ARMV7_LINUX_GNU = "armv7-unknown-linux-gnueabihf";
    /// ARMv6 Linux (kernel 3.2, glibc 2.17)
    const TARGET_ARMV6_LINUX_GNU = "arm-unknown-linux-gnueabi";
    /// ARMv6 Linux, hardfloat (kernel 3.2, glibc 2.17)
    const TARGET_ARMV6_LINUX_GNU_HARDFLOAT = "arm-unknown-linux-gnueabihf";
    /// PowerPC Linux (kernel 3.2, glibc 2.17)
    const TARGET_PPC_LINUX_GNU = "powerpc-unknown-linux-gnu";
    /// PPC64 Linux (kernel 3.2, glibc 2.17)
    const TARGET_PPC64_LINUX_GNU = "powerpc64-unknown-linux-gnu";
    /// PPC64LE Linux (kernel 3.10, glibc 2.17)
    const TARGET_PPC64LE_LINUX_GNU = "powerpc64le-unknown-linux-gnu";
    /// S390x Linux (kernel 3.2, glibc 2.17)
    const TARGET_S390X_LINUX_GNU = "s390x-unknown-linux-gnu";
    /// RISC-V Linux (kernel 4.20, glibc 2.29)
    const TARGET_RISCV_LINUX_GNU = "riscv64gc-unknown-linux-gnu";
    /// LoongArch64 Linux, LP64D ABI (kernel 5.19, glibc 2.36)
    const TARGET_LOONGARCH64_LINUX_GNU = "loongarch64-unknown-linux-gnu";
    /// SPARC Linux (kernel 4.4, glibc 2.23)
    const TARGET_SPARC64_LINUX_GNU = "sparc64-unknown-linux-gnu";
);

/// List of all recognized Linux glibc targets
pub const KNOWN_LINUX_GNU_TARGETS: &[&TargetTripleRef] = &[
    TARGET_X86_LINUX_GNU,
    TARGET_X64_LINUX_GNU,
    TARGET_ARM64_LINUX_GNU,
    TARGET_ARMV7_LINUX_GNU,
    TARGET_ARMV6_LINUX_GNU,
    TARGET_ARMV6_LINUX_GNU_HARDFLOAT,
    TARGET_PPC64_LINUX_GNU,
    TARGET_PPC64LE_LINUX_GNU,
    TARGET_S390X_LINUX_GNU,
    TARGET_RISCV_LINUX_GNU,
    TARGET_LOONGARCH64_LINUX_GNU,
    TARGET_SPARC64_LINUX_GNU,
];

define_target_triples!(
    /// 32-bit Linux with MUSL
    const TARGET_X86_LINUX_MUSL = "i686-unknown-linux-musl";
    /// 64-bit Linux with MUSL
    const TARGET_X64_LINUX_MUSL = "x86_64-unknown-linux-musl";
    /// ARM64 Linux with MUSL
    const TARGET_ARM64_LINUX_MUSL = "aarch64-unknown-linux-musl";
    /// ARMv7-A Linux with MUSL, hardfloat
    const TARGET_ARMV7_LINUX_MUSL = "armv7-unknown-linux-musleabihf";
    /// ARMv6 Linux with MUSL
    const TARGET_ARMV6_LINUX_MUSL = "arm-unknown-linux-musleabi";
    /// ARMv6 Linux with MUSL, hardfloat
    const TARGET_ARMV6_LINUX_MUSL_HARDFLOAT = "arm-unknown-linux-musleabihf";
    /// PowerPC Linux with MUSL
    const TARGET_PPC_LINUX_MUSL = "powerpc-unknown-linux-musl";
    /// PPC64 Linux with MUSL
    const TARGET_PPC64_LINUX_MUSL = "powerpc64-unknown-linux-musl";
    /// PPC64LE Linux with MUSL
    const TARGET_PPC64LE_LINUX_MUSL = "powerpc64le-unknown-linux-musl";
    /// S390x Linux with MUSL
    const TARGET_S390X_LINUX_MUSL = "s390x-unknown-linux-musl";
    /// RISC-V Linux with MUSL
    const TARGET_RISCV_LINUX_MUSL = "riscv64gc-unknown-linux-musl";
    /// LoongArch64 Linux with MUSL, LP64D ABI
    const TARGET_LOONGARCH64_LINUX_MUSL = "loongarch64-unknown-linux-musl";
    /// SPARC Linux with MUSL
    const TARGET_SPARC64_LINUX_MUSL = "sparc64-unknown-linux-musl";
);

/// List of all recognized Linux MUSL targets
pub const KNOWN_LINUX_MUSL_TARGETS: &[&TargetTripleRef] = &[
    TARGET_X86_LINUX_MUSL,
    TARGET_X64_LINUX_MUSL,
    TARGET_ARM64_LINUX_MUSL,
    TARGET_ARMV7_LINUX_MUSL,
    TARGET_ARMV6_LINUX_MUSL,
    TARGET_ARMV6_LINUX_MUSL_HARDFLOAT,
    TARGET_PPC64_LINUX_MUSL,
    TARGET_PPC64LE_LINUX_MUSL,
    TARGET_S390X_LINUX_MUSL,
    TARGET_RISCV_LINUX_MUSL,
    TARGET_LOONGARCH64_LINUX_MUSL,
    TARGET_SPARC64_LINUX_MUSL,
];

/// List of all recognized Linux targets
pub const KNOWN_LINUX_TARGETS: &[&[&TargetTripleRef]] =
    &[KNOWN_LINUX_GNU_TARGETS, KNOWN_LINUX_MUSL_TARGETS];

define_target_triples!(
    /// 64-bit FreeBSD
    const TARGET_X64_FREEBSD = "x86_64-unknown-freebsd";
    /// illumos
    const TARGET_X64_ILLUMOS = "x86_64-unknown-illumos";
    /// NetBSD/amd64
    const TARGET_X64_NETBSD = "x86_64-unknown-netbsd";
    /// ARM64 iOS
    const TARGET_ARM64_IOS = "aarch64-apple-ios";
    /// ARM64 Fuchsia
    const TARGET_ARM64_FUCHSIA = "aarch64-unknown-fuchsia";
    /// ARM64 Android
    const TARGET_ARM64_ANDROID = "aarch64-linux-android";
    /// 64-bit x86 Android
    const TARGET_X64_ANDROID = "x86_64-linux-android";
    /// WebAssembly with WASI
    const TARGET_WASM32_WASI = "wasm32-wasi";
    /// WebAssembly
    const TARGET_WASM32 = "wasm32-unknown-unknown";
    /// SPARC Solaris 10/11, illumos
    const TARGET_SPARC_SOLARIS = "sparcv9-sun-solaris";
    /// 64-bit Solaris 10/11, illumos
    const TARGET_X64_SOLARIS = "x86_64-pc-solaris";
);

/// List of all recognized Other targets
pub const KNOWN_OTHER_TARGETS: &[&TargetTripleRef] = &[
    TARGET_X64_FREEBSD,
    TARGET_X64_ILLUMOS,
    TARGET_X64_NETBSD,
    TARGET_ARM64_IOS,
    TARGET_ARM64_FUCHSIA,
    TARGET_ARM64_ANDROID,
    TARGET_X64_ANDROID,
    TARGET_WASM32_WASI,
    TARGET_WASM32,
    TARGET_SPARC_SOLARIS,
    TARGET_X64_SOLARIS,
];

/// List of all recognized targets
pub const KNOWN_TARGET_TRIPLES: &[&[&TargetTripleRef]] = &[
    KNOWN_WINDOWS_TARGETS,
    KNOWN_MAC_TARGETS,
    KNOWN_LINUX_GNU_TARGETS,
    KNOWN_LINUX_MUSL_TARGETS,
    KNOWN_OTHER_TARGETS,
];

/// The current host target (the target of the machine this code is running on).
/// This is determined through `std::env::consts::OS` rather than running `cargo`
pub const TARGET_HOST: &TargetTripleRef = TargetTripleRef::from_str(std::env::consts::OS);

#[cfg(test)]
mod tests;
